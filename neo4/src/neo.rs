use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    convert::TryInto,
    env::var,
    hash::{Hash, Hasher},
    net::UdpSocket,
    sync::{
        atomic::{AtomicU32, Ordering::SeqCst},
        Arc,
    },
    thread::spawn,
    time::Duration,
};

use bincode::Options;
use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};
use rand::{thread_rng, Rng};
use secp256k1::{hashes::sha256, PublicKey, Secp256k1, VerifyOnly};
use serde::{Deserialize, Serialize};
use tokio::{sync::oneshot, time::Instant};

use crate::{
    common::{ClientTable, Reorder},
    crypto::{verify_message, CryptoMessage, Signature},
    meta::{
        digest, ClientId, Config, Digest, OpNumber, ReplicaId, RequestNumber, ViewNumber,
        ENTRY_NUMBER, MULTICAST_CONTROL_RESET_PORT,
    },
    transport::{
        simulated,
        // Destination::{To, ToAll, ToMulticast, ToReplica, ToSelf},
        Destination::{To, ToAll, ToMulticast, ToReplica},
        InboundAction,
        InboundPacket,
        MulticastVariant,
        Node,
        Transport,
        // TransportMessage::{self, Allowed, Signed, Verified},
        TransportMessage::{self, Allowed, Verified},
    },
    App, InvokeResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Request),               // sent by client
    OrderedRequest(OrderedRequest), // received by replica
    MulticastGeneric(MulticastGeneric),
    MulticastVote(MulticastVote),
    Reply(Reply),
    Query(Query),
    QueryReply(QueryReply),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    client_id: ClientId,
    request_number: RequestNumber,
    op: Vec<u8>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct OrderedRequest {
    client_id: ClientId,
    request_number: RequestNumber,
    op: Vec<u8>,
    sequence_number: u32,
    ordering_state: [u8; 32],
    network_signature: Vec<u8>,
    link_hash: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MulticastGeneric {
    votes: Vec<MulticastVote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MulticastVote {
    view_number: ViewNumber,
    sequence_number: u32,
    ordering_state: [u8; 32],
    replica_id: ReplicaId,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    request_number: RequestNumber,
    sequence_number: u32,
    replica_id: ReplicaId,
    result: Vec<u8>,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    sequence_number: u32,
    replica_id: ReplicaId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryReply {
    ordered_request: OrderedRequest,
}

impl CryptoMessage for Message {
    fn signature_mut(&mut self) -> &mut Signature {
        match self {
            Self::MulticastVote(MulticastVote { signature, .. })
            | Self::Reply(Reply { signature, .. }) => signature,
            _ => unreachable!(),
        }
    }

    fn digest(&self) -> Digest {
        if let Self::OrderedRequest(message) = self {
            digest(Self::Request(Request {
                client_id: message.client_id,
                request_number: message.request_number,
                op: message.op.clone(),
            }))
        } else {
            digest(self)
        }
    }
}

impl Message {
    fn has_network_signature(message: &OrderedRequest) -> bool {
        !message.network_signature.iter().all(|&b| b == 0)
    }

    fn multicast_action(variant: MulticastVariant, message: Message) -> InboundAction<Self> {
        match (variant, message) {
            (MulticastVariant::Disabled, _) => unreachable!(),
            // TODO inline-check message signature
            // for now a standard hashing is used as overhead placeholder
            // calculate a SipHash1-3 should not be easier than a HalfSipHash2-4
            (MulticastVariant::HalfSipHash, message @ Message::OrderedRequest(_)) => {
                let mut hasher = DefaultHasher::new();
                message.digest().hash(&mut hasher);
                if hasher.finish() != 0 {
                    InboundAction::Allow(message)
                } else {
                    unreachable!()
                }
            }
            (MulticastVariant::Secp256k1, Message::OrderedRequest(message))
                if Self::has_network_signature(&message) =>
            {
                // selectively verify part of signatures?
                InboundAction::Verify(
                    Message::OrderedRequest(message),
                    Self::verify_ordered_request_secp256k1,
                )
            }
            (MulticastVariant::Secp256k1, message @ Message::OrderedRequest(..)) => {
                let digest = message.digest();
                let mut message = if let Message::OrderedRequest(message) = message {
                    message
                } else {
                    unreachable!()
                };
                message.ordering_state =
                    *Self::ordering_state(&message.link_hash, &digest, message.sequence_number)
                        .as_ref();
                InboundAction::Allow(Message::OrderedRequest(message))
            }
            _ => InboundAction::Block,
        }
    }

    fn ordering_state(
        link_hash: &[u8; 32],
        message: &Digest,
        sequence_number: u32,
    ) -> secp256k1::Message {
        let mut state = [0; 52];
        for (dest, source) in state[0..32].iter_mut().zip(link_hash.iter()) {
            *dest = *source;
        }
        for (dest, source) in state[16..48].iter_mut().zip(message.iter()) {
            *dest ^= *source;
        }
        state[48..52].copy_from_slice(&sequence_number.to_be_bytes()[..]);
        // println!("state {state:x?}");
        secp256k1::Message::from_hashed_data::<sha256::Hash>(&state[..])
    }

    fn verify_ordering_state_secp256k1(state: secp256k1::Message, signature: &[u8; 64]) -> bool {
        thread_local! {
            static SECP: Secp256k1<VerifyOnly> = Secp256k1::verification_only();
        }
        let key = PublicKey::from_slice(&[
            4, 154, 162, 12, 140, 153, 121, 194, 121, 64, 20, 208, 79, 75, 1, 220, 217, 119, 125,
            11, 247, 157, 165, 59, 2, 221, 169, 89, 137, 73, 210, 79, 199, 66, 221, 152, 117, 155,
            43, 181, 242, 193, 152, 79, 132, 16, 9, 116, 132, 160, 208, 37, 244, 81, 129, 212, 46,
            195, 195, 208, 142, 229, 234, 67, 133,
        ])
        .unwrap();
        let mut signature = *signature;
        signature.reverse();
        let mut signature = secp256k1::ecdsa::Signature::from_compact(&signature[..]).unwrap();
        signature.normalize_s();
        SECP.with(|secp| secp.verify_ecdsa(&state, &signature, &key))
            .is_ok()
    }

    fn verify_ordered_request_secp256k1(&mut self, _config: &Config) -> bool {
        let message_digest = self.digest();
        let message = if let Message::OrderedRequest(request) = self {
            request
        } else {
            unreachable!();
        };
        let ordering_state =
            Self::ordering_state(&message.link_hash, &message_digest, message.sequence_number);
        message.ordering_state = *ordering_state.as_ref();
        Self::verify_ordering_state_secp256k1(
            ordering_state,
            &message.network_signature.clone().try_into().unwrap(),
        )
    }

    fn verify_multicast_generic(&mut self, config: &Config) -> bool {
        let message = if let Message::MulticastGeneric(generic) = self {
            generic
        } else {
            unreachable!()
        };
        if message.votes.len() < config.f * 2 + 1 {
            return false;
        }
        for vote in &message.votes {
            if !verify_message(
                &mut Message::MulticastVote(vote.clone()),
                &config.keys[vote.replica_id as usize].public_key(),
            ) {
                return false;
            }
        }
        true
    }
}

pub struct Client {
    transport: Transport<Self>,
    id: ClientId,
    request_number: RequestNumber,
    invoke: Option<Invoke>,
}

struct Invoke {
    request: Request,
    result: Vec<u8>,
    sequence_number: u32,
    speculative_replicas: HashSet<ReplicaId>,
    continuation: oneshot::Sender<Vec<u8>>,
    timer_id: u32,
}

impl Client {
    pub fn new(transport: Transport<Self>) -> Self {
        Self {
            id: transport.create_id(),
            transport,
            request_number: 0,
            invoke: None,
        }
    }
}

impl AsMut<Transport<Self>> for Client {
    fn as_mut(&mut self) -> &mut Transport<Self> {
        &mut self.transport
    }
}

impl crate::Client for Client {
    fn invoke(&mut self, op: &[u8]) -> InvokeResult {
        assert!(self.invoke.is_none());
        self.request_number += 1;
        let request = Request {
            client_id: self.id,
            request_number: self.request_number,
            op: op.to_vec(),
        };
        let (continuation, result) = oneshot::channel();
        self.invoke = Some(Invoke {
            request,
            timer_id: 0,
            continuation,
            result: Vec::new(),
            speculative_replicas: HashSet::new(),
            sequence_number: 0,
        });
        self.send_request();
        Box::pin(async { result.await.unwrap() })
    }
}

impl Node for Client {
    type Message = Message;

    fn receive_message(&mut self, message: TransportMessage<Self::Message>) {
        let message = if let Allowed(Message::Reply(message)) = message {
            message
        } else {
            unreachable!()
        };
        let invoke = if let Some(invoke) = self.invoke.as_mut() {
            invoke
        } else {
            return;
        };
        if message.request_number != invoke.request.request_number {
            return;
        }

        if invoke.speculative_replicas.is_empty() {
            invoke.sequence_number = message.sequence_number;
            invoke.result = message.result.clone();
        } else if (message.sequence_number, &message.result)
            != (invoke.sequence_number, &invoke.result)
        {
            println!("! mismatch sequence number / result");
            return;
        }
        invoke.speculative_replicas.insert(message.replica_id);
        if invoke.speculative_replicas.len() == self.transport.config.f * 2 + 1 {
            let invoke = self.invoke.take().unwrap();
            self.transport.cancel_timer(invoke.timer_id);
            invoke.continuation.send(message.result).unwrap();
        }
    }
}

impl Client {
    fn send_request(&mut self) {
        let request = &self.invoke.as_ref().unwrap().request;
        self.transport
            .send_message(ToMulticast, Message::Request(request.clone()));
        let request_number = request.request_number;
        let on_resend = move |receiver: &mut Self| {
            assert_eq!(
                receiver.invoke.as_ref().unwrap().request.request_number,
                request_number
            );
            println!("! client {} resend request {}", receiver.id, request_number);
            receiver.send_request();
        };
        self.invoke.as_mut().unwrap().timer_id = self
            .transport
            .create_timer(Duration::from_secs(1), on_resend);
    }
}

pub struct Replica {
    transport: Transport<Self>,
    id: ReplicaId,
    view_number: ViewNumber,
    app: Box<dyn App + Send>,
    client_table: ClientTable<Reply>,
    // log state:
    // (TODO committed)
    // speculative committed entries: 0..=speculative number, SpeculativeCommitted
    // current voting batch: speculative number..=vote number, Voting
    // verified and yet to be voted: vote number..=verify number, Voting,
    // fast verifying (see below) verify number..=(last entry), FastVerifying
    //
    // if voting is not enabled, there is always speculative number == verify number
    // vote number is not specified
    log: Vec<LogEntry>,
    verify_number: u32,
    vote_number: u32,
    speculative_number: u32,
    n_speculative: u32,
    query_number: Arc<AtomicU32>,
    n_query: u32,
    n_send_query: Arc<AtomicU32>,

    reorder_ordered_request: Reorder<OrderedRequest>, // received and yet to be reordered

    enable_vote: bool,
    // in fast path all received messages are assumed to be correct despite some
    // of them has no signature. if the received message contains a link hash
    // that matches previous message's ordering state, it is qualified for fast
    // verify, and is appended into log with `FastVerifying` status. whenever a
    // signed ordered request is truly verified (automatically upon received),
    // every message that is already in log is immediately verified
    // the protocol must fall back to slow path whenever there is a new message
    // (with or without a signature) received whose link hash does not match
    // previous message's digest. this means either/both current or/and previous
    // message is incorrect. All logged messages with `FastVerifying` status
    // then have to be moved into a hash map keyed by ordering state (i.e. link
    // hash of successive message) and perform slow verification instead.
    // if network promise to sign every ordered message, then link hash is not
    // required and all messages will bypass verifying stage and directly go
    // into vote buffer. in this case a mismatched link hash will not cause
    // falling back to slow path
    // TODO slow verify buffer
    votes: HashMap<ReplicaId, MulticastVote>,
    pending_votes: HashMap<u32, Vec<MulticastVote>>,
    batch_size: usize,
    // pending_generics: HashMap<u32, Vec<MulticastGeneric>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntry {
    status: LogStatus,
    request: OrderedRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum LogStatus {
    FastVerifying,
    Voting,
    SpeculativeCommitted,
    Committed,
}

impl Replica {
    pub fn new(
        mut transport: Transport<Self>,
        id: ReplicaId,
        app: impl App + Send + 'static,
        enable_vote: bool,
        batch_size: usize,
    ) -> Self {
        // TODO consider avoid side effect in constructor by adding a dedicated
        // initialization method
        transport.create_timer(Duration::ZERO, |node| {
            node.transport.send_raw(
                (
                    node.transport.config.multicast.ip(),
                    MULTICAST_CONTROL_RESET_PORT,
                ),
                &[],
            );
        });
        let query_number = Arc::new(AtomicU32::new(0));
        let n_send_query = Arc::new(AtomicU32::new(0));
        spawn({
            let query_number = query_number.clone();
            let n_send_query = n_send_query.clone();
            let config = transport.config.clone();
            move || {
                let mut cpu_set = CpuSet::new();
                cpu_set.set(14).unwrap();
                sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
                let socket = UdpSocket::bind((config.replicas[id as usize].ip(), 61000)).unwrap();
                let mut now = Instant::now();
                let mut sending = 0;
                loop {
                    while Instant::now() - now
                        < Duration::from_micros(match sending {
                            // 0 => 0,
                            // 1 | 2 | 3 => 100,
                            _ => 40,
                        })
                    {}
                    now = Instant::now();
                    let query_number = query_number.load(SeqCst);
                    // let query_number = query_number.swap(0, SeqCst);
                    if query_number == 0 {
                        sending = 0;
                        continue;
                    }
                    sending += 1;
                    let query = Message::Query(Query {
                        sequence_number: query_number,
                        replica_id: id,
                    });
                    n_send_query.fetch_add(1, SeqCst);
                    let message = bincode::options().serialize(&query).unwrap();
                    if id == 0 {
                        for &replica in &config.replicas[1..] {
                            socket.send_to(&message, replica).unwrap();
                        }
                    } else {
                        socket.send_to(&message, config.replicas[0]).unwrap();
                    }
                }
            }
        });
        Self {
            id,
            view_number: 0,
            app: Box::new(app),
            client_table: ClientTable::default(),
            log: Vec::with_capacity(ENTRY_NUMBER),
            verify_number: 0,
            vote_number: 0,
            speculative_number: 0,
            n_speculative: 0,
            query_number,
            n_query: 0,
            n_send_query,
            reorder_ordered_request: Reorder::new(1),
            enable_vote,
            batch_size,
            votes: HashMap::new(),
            pending_votes: HashMap::new(),
            // pending_generics: HashMap::new(),
            transport,
        }
    }
}

impl AsMut<Transport<Self>> for Replica {
    fn as_mut(&mut self) -> &mut Transport<Self> {
        &mut self.transport
    }
}

impl Node for Replica {
    type Message = Message;

    fn inbound_action(
        &self,
        packet: InboundPacket<'_, Self::Message>,
    ) -> InboundAction<Self::Message> {
        match packet {
            InboundPacket::OrderedMulticast {
                sequence_number,
                signature,
                link_hash,
                message: Message::Request(message),
                ..
            } => {
                let request = OrderedRequest {
                    client_id: message.client_id,
                    request_number: message.request_number,
                    op: message.op,
                    sequence_number,
                    ordering_state: Default::default(),
                    network_signature: signature.to_vec(),
                    link_hash: *link_hash,
                };
                Message::multicast_action(
                    self.transport.multicast_variant(),
                    Message::OrderedRequest(request),
                )
            }
            InboundPacket::Unicast {
                message: Message::MulticastVote(message),
                ..
            } => {
                // if message.sequence_number != self.vote_number {
                //     return InboundAction::Block;
                // }
                let replica_id = message.replica_id;
                InboundAction::VerifyReplica(Message::MulticastVote(message), replica_id)
            }
            InboundPacket::Unicast {
                message: Message::MulticastGeneric(message),
                ..
            } => InboundAction::Verify(
                Message::MulticastGeneric(message),
                Message::verify_multicast_generic,
            ),
            InboundPacket::Unicast {
                message: message @ Message::Query(_),
            } => InboundAction::Allow(message),
            InboundPacket::Unicast {
                message: Message::QueryReply(QueryReply { ordered_request }),
            } => {
                if ordered_request.sequence_number == self.pending_number() {
                    Message::multicast_action(
                        self.transport.multicast_variant(),
                        Message::OrderedRequest(ordered_request),
                    )
                } else {
                    InboundAction::Block
                }
            }
            _ => InboundAction::Block,
        }
    }

    fn receive_message(&mut self, message: TransportMessage<Self::Message>) {
        match message {
            Verified(Message::OrderedRequest(message))
            | Allowed(Message::OrderedRequest(message)) => self.handle_ordered_request(message),
            Verified(Message::MulticastVote(message)) => self.handle_multicast_vote(message),
            // need to be caution to reuse...
            // Signed(Message::MulticastVote(message)) => self.handle_multicast_vote(message),
            // Verified(Message::MulticastGeneric(message)) => self.handle_multicast_generic(message),
            Allowed(Message::Query(message)) => self.handle_query(message),
            _ => unreachable!(),
        }
    }
}

impl Replica {
    fn pending_number(&self) -> u32 {
        self.log.len() as u32 + 1
    }

    fn handle_ordered_request(&mut self, message: OrderedRequest) {
        if message.sequence_number < self.pending_number() {
            return; // resolved query
        }

        // we don't look up client table at this point, because every ordered
        // request will be assigned to different sequence number even if they
        // contains identical client request. so certain states e.g. reordering
        // buffer must be updated before at most once semantic effects
        // just treat it as the message is still in network stack

        let mut ordered = self
            .reorder_ordered_request
            .insert_reorder(message.sequence_number, message);
        while let Some(request) = ordered {
            self.verify_ordered_request(request);
            ordered = self.reorder_ordered_request.expect_next();
        }
        // if !self.reorder_ordered_request.is_empty() {
        if self.reorder_ordered_request.len() > 50 {
            let prev = self.query_number.swap(self.pending_number(), SeqCst);
            if prev != self.pending_number() {
                self.n_query += 1;
            }
        } else {
            self.query_number.swap(0, SeqCst);
        }
    }

    fn verify_ordered_request(&mut self, request: OrderedRequest) {
        let link_hash = self
            .log
            .last()
            .map(|entry| entry.request.ordering_state)
            .unwrap_or_default();
        if !Message::has_network_signature(&request) {
            if request.link_hash == link_hash {
                self.log.push(LogEntry {
                    request,
                    status: LogStatus::FastVerifying,
                });
                return;
            }
            todo!() // fallback to slow path
        }

        if self.verify_number < request.sequence_number - 1 {
            for entry in self.log.get_mut(self.verify_number as usize..).unwrap() {
                assert_eq!(entry.status, LogStatus::FastVerifying);
                entry.status = LogStatus::Voting;
            }
        }
        self.verify_number = request.sequence_number;
        self.log.push(LogEntry {
            status: LogStatus::Voting,
            request,
        });

        if !self.enable_vote {
            self.speculative_commit(self.verify_number);
            return;
        }

        if self.transport.multicast_variant() == MulticastVariant::HalfSipHash {
            let entry = self.log.last_mut().unwrap();
            entry.request.link_hash = link_hash;
            // (link hash, sequence number, Request) all included in this digest
            entry.request.ordering_state = digest(&entry.request);
        }

        if self.speculative_number == self.vote_number
            || self.verify_number >= self.vote_number + self.batch_size as u32
        {
            self.send_vote(self.verify_number);
        }

        // assert verify number == request sequence number
        if let Some(messages) = self.pending_votes.remove(&self.verify_number) {
            for message in messages {
                self.handle_multicast_vote(message);
            }
        }
        // if let Some(messages) = self.pending_generics.remove(&self.verify_number) {
        //     for message in messages {
        //         self.handle_multicast_generic(message);
        //     }
        // }
    }

    fn handle_multicast_vote(&mut self, message: MulticastVote) {
        // TODO assert is primary
        assert!(self.enable_vote);

        if message.sequence_number <= self.speculative_number {
            return;
        }

        let entry = if let Some(entry) = self.log.get((message.sequence_number - 1) as usize) {
            entry
        } else {
            //
            self.pending_votes
                .entry(message.sequence_number)
                .or_default()
                .push(message);
            return;
        };
        if entry.request.ordering_state != message.ordering_state {
            println!("! mismatch ordering state in vote");
            return;
        }

        self.votes.insert(message.replica_id, message);
        // if self.votes.len() < self.transport.config.f * 2 + 1 {
        if self.votes.len() < self.transport.config.f * 2 {
            return;
        }
        // can we do better?
        let mut voted_numbers = self
            .votes
            .values()
            .map(|vote| vote.sequence_number)
            .collect::<Vec<_>>();

        voted_numbers.push(self.verify_number); //

        voted_numbers.resize(self.transport.config.f * 3 + 1, 0);
        let (_, &mut voted_number, _) = voted_numbers.select_nth_unstable(self.transport.config.f);
        assert!(voted_number >= self.speculative_number);
        if voted_number == self.speculative_number {
            return;
        }

        // let generic = MulticastGeneric {
        //     votes: self
        //         .votes
        //         .values()
        //         .filter(|vote| vote.sequence_number >= voted_number)
        //         .take(self.transport.config.f * 2 + 1)
        //         .cloned()
        //         .collect(),
        // };
        // assert_eq!(generic.votes.len(), self.transport.config.f * 2 + 1);
        // self.transport
        //     .send_message(ToAll, Message::MulticastGeneric(generic));
        if self.vote_number < self.verify_number {
            self.send_vote(self.verify_number);
        }

        self.speculative_commit(voted_number);
    }

    fn speculative_commit(&mut self, speculative_number: u32) {
        self.n_speculative += 1;
        if self
            .log
            .get(speculative_number as usize - 1)
            .filter(|entry| entry.status == LogStatus::Voting)
            .is_none()
        {
            todo!("speculative commit up to {speculative_number}") // query missing ordered requests
        }

        for entry in self
            .log
            .get_mut(self.speculative_number as usize..=speculative_number as usize - 1)
            .unwrap()
        {
            assert_eq!(entry.status, LogStatus::Voting);
            entry.status = LogStatus::SpeculativeCommitted;
            if let Some(resend) = self
                .client_table
                .insert_prepare(entry.request.client_id, entry.request.request_number)
            {
                resend(|reply| {
                    // self.transport.send_signed_message(
                    //     To(entry.request.client_id.0),
                    //     Message::Reply(reply),
                    //     self.id,
                    // )
                    self.transport
                        .send_message(To(entry.request.client_id.0), Message::Reply(reply))
                });
            } else {
                let op_number = entry.request.sequence_number as OpNumber;
                let result = self.app.replica_upcall(op_number, &entry.request.op);
                let reply = Reply {
                    request_number: entry.request.request_number,
                    result,
                    sequence_number: entry.request.sequence_number,
                    replica_id: self.id,
                    signature: Signature::default(),
                };
                self.client_table.insert_commit(
                    entry.request.client_id,
                    entry.request.request_number,
                    reply.clone(),
                );
                // self.transport.send_signed_message(
                //     To(entry.request.client_id.0),
                //     Message::Reply(reply),
                //     self.id,
                // );
                self.transport
                    .send_message(To(entry.request.client_id.0), Message::Reply(reply));
            }
        }
        self.speculative_number = speculative_number;
    }

    // fn handle_multicast_generic(&mut self, message: MulticastGeneric) {
    //     assert!(self.enable_vote);
    //     let mut voted_number = u32::MAX;
    //     for vote in &message.votes {
    //         if vote.replica_id == self.id {
    //             continue;
    //         }
    //         let entry = if let Some(entry) = self.log.get((vote.sequence_number - 1) as usize) {
    //             entry
    //         } else {
    //             //
    //             self.pending_generics
    //                 .entry(vote.sequence_number)
    //                 .or_default()
    //                 .push(message);
    //             return;
    //         };
    //         if entry.request.ordering_state != vote.ordering_state {
    //             println!("! mismatch ordering state in generic");
    //             return;
    //         }
    //         voted_number = u32::min(voted_number, vote.sequence_number);
    //     }
    //     if voted_number <= self.speculative_number {
    //         return;
    //     }

    //     // assert verify number >= vote number?
    //     if self.verify_number > voted_number {
    //         self.send_vote(self.verify_number);
    //     }
    //     self.speculative_commit(voted_number);
    // }

    fn send_vote(&mut self, vote_number: u32) {
        let message = MulticastVote {
            view_number: self.view_number,
            sequence_number: vote_number,
            ordering_state: self.log[(self.verify_number - 1) as usize]
                .request
                .ordering_state,
            replica_id: self.id,
            signature: Signature::default(),
        };
        // let primary = self.transport.config.primary(self.view_number);
        self.transport.send_signed_message(
            // if self.id == primary {
            //     ToSelf
            // } else {
            //     ToReplica(primary)
            // },
            ToAll,
            Message::MulticastVote(message),
            self.id,
        );
        self.vote_number = vote_number;
    }

    fn handle_query(&mut self, message: Query) {
        if let Some(entry) = self.log.get((message.sequence_number - 1) as usize) {
            self.transport.send_message(
                ToReplica(message.replica_id),
                Message::QueryReply(QueryReply {
                    ordered_request: entry.request.clone(),
                }),
            );
        }
    }
}

impl Drop for Replica {
    fn drop(&mut self) {
        println!(
            "average speculative size {}",
            self.speculative_number as f32 / self.n_speculative as f32
        );
        println!("queried {}", self.n_query);
        println!("send query {}", self.n_send_query.load(SeqCst));
        if self.id != self.transport.config.primary(self.view_number) {
            return;
        }
        if !self.log.is_empty() {
            let signed_count = self
                .log
                .iter()
                .filter(|entry| Message::has_network_signature(&entry.request))
                .count();
            println!(
                "network signature batch size {}",
                self.log.len() as f32 / signed_count as f32
            );
        }
    }
}

struct Switch<T = simulated::BasicSwitch> {
    sequence_number: u32,
    config: Config,
    underlying: T,
}

impl<T: AsMut<simulated::BasicSwitch>> AsMut<simulated::BasicSwitch> for Switch<T> {
    fn as_mut(&mut self) -> &mut simulated::BasicSwitch {
        self.underlying.as_mut()
    }
}

impl<T> simulated::Switch for Switch<T>
where
    T: simulated::Switch + AsRef<simulated::BasicSwitch>,
{
    fn handle_packet(&mut self, mut packet: simulated::Packet) {
        let multicast = self.config.multicast;
        if packet.destination == (multicast.ip(), multicast.port() + 1).into() {
            self.sequence_number = 0;
            return;
        }
        if packet.destination != multicast {
            return self.underlying.handle_packet(packet);
        }

        self.sequence_number += 1;
        let n = self.sequence_number;
        if var("NEO_NETLOG").as_deref().unwrap_or("0") != "0" {
            println!(
                "* [{:6?}] [{} -> <multicast>] sequence {n} message length {}",
                Instant::now() - self.underlying.as_ref().epoch,
                packet.source,
                packet.buffer[100..].len()
            );
        }
        packet.buffer[0..4].copy_from_slice(&n.to_be_bytes()[..]);
        // this is synchronized with switch program
        packet.buffer[7] = (n & 0xff) as u8;
        packet.buffer[8] = ((n + 1) & 0xff) as u8;

        // currently implementation is not good at handle multicast that varies
        // to much on arriving time, so simply unify it here
        let delay = Duration::from_millis(thread_rng().gen_range(1..10));
        // TODO more elegant than clone addresses
        for &destination in &self.config.replicas {
            self.underlying.handle_packet(simulated::Packet {
                source: packet.source,
                destination,
                buffer: packet.buffer.clone(),
                delay,
                multicast_outgress: true,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{task::yield_now, time::timeout};

    use crate::{
        common::TestApp,
        crypto::Executor,
        meta::ReplicaId,
        neo::Message,
        transport::{
            simulated::{BasicSwitch, Network},
            Concurrent,
            MulticastVariant::HalfSipHash,
            Run, Transport,
        },
        Client as _,
    };

    use super::{Client, Replica, Switch};

    struct System {
        net: Concurrent<Network<Switch>>,
        replicas: Vec<Concurrent<Replica>>,
        clients: Vec<Client>,
    }

    impl System {
        async fn new(num_client: usize) -> Self {
            let config = Network::config(4, 1);
            let mut net = Network(Switch {
                sequence_number: 0,
                config: config.clone(),
                underlying: BasicSwitch::default(),
            });
            let clients = (0..num_client)
                .map(|i| {
                    Client::new(Transport::new(
                        config.clone(),
                        net.insert_socket(Network::client(i)),
                        Executor::Inline,
                    ))
                })
                .collect::<Vec<_>>();
            let replicas = (0..4)
                .map(|i| {
                    let mut transport = Transport::new(
                        config.clone(),
                        net.insert_socket(config.replicas[i]),
                        Executor::Inline,
                    );
                    transport
                        .listen_multicast(net.multicast_listener(config.replicas[i]), HalfSipHash);
                    let replica =
                        Replica::new(transport, i as ReplicaId, TestApp::default(), true, 1);
                    Concurrent::run(replica)
                })
                .collect::<Vec<_>>();

            let system = Self {
                net: Concurrent::run(net),
                replicas,
                clients,
            };
            yield_now().await;
            system
        }
    }

    #[tokio::test(start_paused = true)]
    async fn single_op() {
        let mut system = System::new(1).await;
        let result = system.clients[0].invoke("hello".as_bytes());
        timeout(
            Duration::from_millis(40),
            system.clients[0].run(async {
                assert_eq!(&result.await, "[1] hello".as_bytes());
            }),
        )
        .await
        .unwrap();

        let mut commit_count = 1;
        for (i, replica) in system.replicas.into_iter().enumerate() {
            if i == 0 {
                assert_eq!(replica.join().await.log.len(), 1);
            } else {
                commit_count += replica.join().await.log.len();
            }
        }
        assert!(commit_count >= 3);
        system.net.join().await;
    }

    #[test]
    fn verify_secp256k1() {
        let payload_digest = [
            243, 212, 139, 81, 238, 147, 91, 10, 96, 155, 86, 225, 100, 38, 67, 64, 228, 202, 178,
            31, 88, 243, 90, 205, 67, 42, 27, 60, 57, 69, 71, 63,
        ];
        let state = Message::ordering_state(&[0; 32], &payload_digest, 1);
        let expected_state = [
            80, 26, 28, 235, 101, 124, 189, 202, 190, 170, 121, 73, 120, 209, 62, 117, 93, 73, 219,
            53, 156, 66, 38, 11, 174, 131, 19, 221, 129, 61, 11, 146,
        ];
        assert_eq!(state.as_ref(), &expected_state);
        let signature = [
            187, 229, 243, 76, 218, 46, 223, 84, 155, 159, 249, 114, 135, 87, 52, 81, 202, 213, 2,
            41, 184, 201, 172, 46, 170, 247, 122, 8, 201, 206, 124, 184, 249, 54, 224, 188, 19,
            241, 1, 134, 176, 153, 111, 131, 69, 200, 49, 181, 41, 82, 157, 248, 133, 79, 52, 73,
            16, 195, 88, 146, 1, 138, 48, 249,
        ];
        assert!(Message::verify_ordering_state_secp256k1(state, &signature));
    }

    #[test]
    fn verify_secp256k1_2() {
        let payload_digest = [
            227, 238, 185, 14, 243, 23, 132, 185, 42, 63, 187, 238, 71, 67, 169, 16, 220, 7, 231,
            233, 193, 140, 136, 215, 174, 56, 126, 102, 144, 169, 160, 246,
        ];
        let state = Message::ordering_state(&[0; 32], &payload_digest, 1);
        let expected_state = [
            6, 115, 62, 115, 60, 67, 6, 7, 8, 84, 128, 248, 174, 37, 68, 182, 249, 53, 139, 216,
            20, 13, 12, 177, 52, 6, 90, 121, 7, 193, 176, 247,
        ];
        assert_eq!(state.as_ref(), &expected_state);
        let signature = [
            50, 114, 117, 36, 14, 241, 10, 44, 125, 236, 231, 154, 189, 231, 130, 218, 138, 130,
            201, 58, 157, 33, 144, 156, 19, 101, 18, 80, 246, 217, 239, 159, 249, 54, 224, 188, 19,
            241, 1, 134, 176, 153, 111, 131, 69, 200, 49, 181, 41, 82, 157, 248, 133, 79, 52, 73,
            16, 195, 88, 146, 1, 138, 48, 249,
        ];
        assert!(Message::verify_ordering_state_secp256k1(state, &signature));
    }
}
