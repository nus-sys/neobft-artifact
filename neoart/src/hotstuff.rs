use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{
    common::ClientTable,
    crypto::{verify_message, CryptoMessage, Signature},
    meta::{digest, ClientId, Config, Digest, OpNumber, ReplicaId, RequestNumber, ENTRY_NUMBER},
    transport::{
        Destination::{To, ToAll, ToReplica, ToSelf},
        InboundAction,
        InboundPacket::Unicast,
        Node, Transport,
        TransportMessage::{self, Allowed, Signed, Verified},
    },
    App, InvokeResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Block {
    parent_hash: Digest,
    requests: Vec<Request>,
    quorum_certificate: QuorumCertificate,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct QuorumCertificate {
    object_hash: Digest,
    signatures: Vec<(ReplicaId, Signature)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    Reply(Reply),
    Proposal(Proposal),
    Vote(Vote),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Request {
    client_id: ClientId,
    request_number: RequestNumber,
    op: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    request_number: RequestNumber,
    result: Vec<u8>,
    replica_id: ReplicaId,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    requests: Vec<Request>,
    parent_hash: Digest,
    certified_hash: Digest,
    quorum_certificate: Vec<(ReplicaId, Signature)>,
    proposer: ReplicaId,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    block_hash: Digest,
    voter: ReplicaId,
    signature: Signature,
}

impl CryptoMessage for Message {
    fn signature_mut(&mut self) -> &mut Signature {
        match self {
            Self::Request(_) => unreachable!(),
            Self::Reply(Reply { signature, .. })
            | Self::Vote(Vote { signature, .. })
            | Self::Proposal(Proposal { signature, .. }) => signature,
        }
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
    replied_replicas: HashSet<ReplicaId>,
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
            replied_replicas: HashSet::new(),
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

        if invoke.replied_replicas.is_empty() {
            invoke.result = message.result.clone();
        } else if message.result != invoke.result {
            println!("! mismatch result");
            return;
        }
        invoke.replied_replicas.insert(message.replica_id);
        if invoke.replied_replicas.len() == self.transport.config.f + 1 {
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
            .send_message(ToAll, Message::Request(request.clone()));
        // self.transport
        //     .send_message(ToReplica(0), Message::Request(request.clone()));
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

type BlockId = usize;
pub struct Replica {
    // TODO fetch context
    transport: Transport<Self>,
    app: Box<dyn App + Send>,
    // block hash => list of messages that cannot make progress without keyed
    // block exist
    // the explicit state of libhotstuff's `blk_fetch_waiting` and
    // `blk_deliver_waiting`, which implicit construct this state with varaible
    // capturing of closures registered to promises
    waiting_messages: HashMap<Digest, Vec<Message>>,
    client_table: ClientTable<Reply>,
    pending_requests: Vec<Request>,

    // block0: usize, // fixed zero
    block_proposal: BlockId,
    block_lock: BlockId,
    block_commit: BlockId,
    block_execute: BlockId,
    voted_height: OpNumber,
    high_quorum_certificate: BlockId,
    // libhotstuff uses ordered `std::set`, i don't see why
    // tails: HashSet<BlockId>,
    id: ReplicaId,
    // command cache is never utilize in libhotstuff so omit it
    storage: Storage,

    propose_parent: BlockId,
    manual_round: u32,
    quorum_certificate_finished: bool,

    profile: Profile,
}

#[derive(Default)]
struct Profile {
    enter_request: Vec<Instant>,
    exit_request: Vec<Instant>,
    send_proposal: Vec<Instant>,
    quorum_certificate_finish: Vec<Instant>,
}

#[derive(Default)]
struct StorageBlock {
    requests: Vec<Request>,
    hash: Digest, // reverse index of `block_ids`
    height: OpNumber,
    status: BlockStatus,
    parent_hash: Digest,
    parent: BlockId,
    certified: BlockId,
    certified_hash: Digest,
    // the `self_quorum_certificate` in libhotstuff
    // seems like "self" means this is the QC for this block "itself", different
    // from the reference above
    // since the two elements have very different types here the `self_` is not
    // very necessary
    // notice: reusing `QuorumCertificate` struct as container, which may not
    // be fully collected to become a valid QC (yet)
    quorum_certificate: Option<Vec<(ReplicaId, Signature)>>,
    voted: HashSet<ReplicaId>,
}

struct Storage {
    arena: Vec<StorageBlock>,
    block_ids: HashMap<Digest, BlockId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum BlockStatus {
    Delivering,
    Deciding,
    Decided,
}

impl Default for BlockStatus {
    fn default() -> Self {
        Self::Delivering
    }
}

impl Replica {
    const BLOCK_GENESIS: BlockId = 0;
    pub fn new(transport: Transport<Self>, id: ReplicaId, app: impl App + Send + 'static) -> Self {
        let mut arena = Vec::with_capacity(ENTRY_NUMBER);
        // genesis
        arena.push(StorageBlock {
            height: 0,
            status: BlockStatus::Decided,
            quorum_certificate: Some(Vec::new()),
            ..StorageBlock::default()
        });
        let mut block_ids = HashMap::with_capacity(ENTRY_NUMBER);
        block_ids.insert(Digest::default(), Self::BLOCK_GENESIS);
        Self {
            transport,
            app: Box::new(app),
            waiting_messages: HashMap::new(),
            client_table: ClientTable::default(),
            pending_requests: Vec::new(),
            block_proposal: Self::BLOCK_GENESIS,
            block_lock: Self::BLOCK_GENESIS,
            block_commit: Self::BLOCK_GENESIS,
            block_execute: Self::BLOCK_GENESIS,
            voted_height: 0,
            high_quorum_certificate: Self::BLOCK_GENESIS,
            id,
            storage: Storage { arena, block_ids },
            propose_parent: Self::BLOCK_GENESIS,
            manual_round: 0,
            quorum_certificate_finished: true,

            profile: Profile::default(),
        }
    }
}

impl AsMut<Transport<Self>> for Replica {
    fn as_mut(&mut self) -> &mut Transport<Self> {
        &mut self.transport
    }
}

impl Message {
    fn verify_proposal(&mut self, config: &Config) -> bool {
        // if let Self::Proposal(proposal) = self {
        //     let id = proposal.proposer;
        //     if !verify_message(self, &config.keys[id as usize].public_key()) {
        //         return false;
        //     }
        // } else {
        //     unreachable!();
        // }
        let proposal = if let Self::Proposal(proposal) = self {
            proposal
        } else {
            unreachable!()
        };
        // genesis
        if proposal.certified_hash == Digest::default() {
            return true;
        }
        let signatures = &proposal.quorum_certificate;
        if signatures.len() < config.n - config.f {
            return false;
        }
        for &(replica_id, signature) in signatures {
            if !verify_message(
                &mut Message::Vote(Vote {
                    voter: replica_id,
                    block_hash: proposal.certified_hash,
                    signature,
                }),
                &config.keys[replica_id as usize].public_key(),
            ) {
                return false;
            }
        }
        true
    }
}

impl Node for Replica {
    type Message = Message;
    fn inbound_action(
        &self,
        packet: crate::transport::InboundPacket<'_, Self::Message>,
    ) -> InboundAction<Self::Message> {
        let message = if let Unicast { message } = packet {
            message
        } else {
            return InboundAction::Block;
        };
        match message {
            // Message::Request(_) => {
            //     if self.id == self.get_proposer() {
            //         InboundAction::Allow(message)
            //     } else {
            //         InboundAction::Block
            //     }
            // }
            Message::Request(_) => InboundAction::Allow(message),
            Message::Proposal(_) => InboundAction::Verify(message, Message::verify_proposal),
            Message::Vote(Vote { voter, .. }) => InboundAction::VerifyReplica(message, voter),
            _ => InboundAction::Block,
        }
    }
    fn receive_message(&mut self, message: TransportMessage<Self::Message>) {
        match message {
            Allowed(Message::Request(message)) => {
                self.profile.enter_request.push(Instant::now());
                self.handle_request(message);
                self.profile.exit_request.push(Instant::now());
            }
            Verified(Message::Proposal(message)) => self.handle_proposal(message),
            Signed(Message::Proposal(message)) => {
                self.transport
                    .send_message(ToAll, Message::Proposal(message.clone()));
                self.on_receive_proposal(message, self.block_proposal);
            }
            // careful
            Verified(Message::Vote(message)) | Signed(Message::Vote(message)) => {
                self.handle_vote(message)
            }
            _ => unreachable!(),
        }
    }
}

impl Replica {
    fn handle_request(&mut self, message: Request) {
        if let Some(resend) = self
            .client_table
            .insert_prepare(message.client_id, message.request_number)
        {
            resend(|reply| {
                println!("! resend");
                // self.transport.send_signed_message(
                //     To(message.client_id.0),
                //     Message::Reply(reply),
                //     self.id,
                // );
                self.transport
                    .send_message(To(message.client_id.0), Message::Reply(reply));
            });
            return;
        }
        if self.id != self.get_proposer() {
            return;
        }
        self.pending_requests.push(message);
        self.beat();
    }

    // the beat strategy is modified (mostly simplified), to prevent introduce
    // timeout on critical path when concurrent request number is less than
    // batch size
    // in this implementation it is equivalent to next proposing or manual
    // rounds start immediately after new QC get collected
    const MAX_BATCH: usize = 1000;
    fn beat(&mut self) {
        // TODO rotating
        if !self.quorum_certificate_finished {
            return;
        }
        if !self.pending_requests.is_empty() {
            self.manual_round = 0;
        } else {
            if self.manual_round == 3 {
                return;
            }
            self.manual_round += 1;
        }

        self.quorum_certificate_finished = false;
        let requests = self
            .pending_requests
            .drain(..usize::min(Self::MAX_BATCH, self.pending_requests.len()))
            .collect();
        let parent = self.get_parent();
        self.on_propose(requests, parent);
    }

    fn handle_proposal(&mut self, message: Proposal) {
        let block = self.add_block(StorageBlock {
            requests: message.requests.clone(),
            parent_hash: message.parent_hash,
            certified_hash: message.certified_hash,
            ..Default::default()
        });
        if !self.is_block_delivered(&message.parent_hash) {
            println!("! message pending deliver");
            self.waiting_messages
                .entry(message.parent_hash)
                .or_default()
                .push(Message::Proposal(message));
        } else if !self.storage.block_ids.contains_key(&message.certified_hash) {
            unreachable!("expect QC delivered equal or earlier than parent");
        } else {
            let valid = self.on_deliver_block(block);
            assert!(valid);

            let certified = self.storage.arena[block].certified;
            self.storage.arena[certified].quorum_certificate =
                Some(message.quorum_certificate.clone());

            self.on_receive_proposal(message, block);

            if let Some(messages) = self
                .waiting_messages
                .remove(&self.storage.arena[block].hash)
            {
                for message in messages {
                    self.receive_message(Verified(message)); // careful
                }
            }
        }
    }

    fn handle_vote(&mut self, message: Vote) {
        if self.is_block_delivered(&message.block_hash) {
            self.on_receive_vote(message);
        } else {
            self.waiting_messages
                .entry(message.block_hash)
                .or_default()
                .push(Message::Vote(message));
        }
    }

    fn on_propose(&mut self, requests: Vec<Request>, parent: BlockId) -> BlockId {
        // self.core.tails.remove(&parent);
        let parent_hash = self.storage.arena[parent].hash;
        let certified_hash = self.storage.arena[self.high_quorum_certificate].hash;
        let block_new = self.add_block(StorageBlock {
            requests: requests.clone(),
            height: self.storage.arena[parent].height + 1,
            parent,
            parent_hash,
            certified: self.high_quorum_certificate,
            certified_hash,
            quorum_certificate: Some(Vec::new()),
            status: BlockStatus::Deciding,
            ..Default::default()
        });
        // all initialized above already
        // self.on_deliver_block(block_new);
        self.update(block_new);

        let proposal = Proposal {
            requests,
            proposer: self.id,
            parent_hash,
            certified_hash,
            quorum_certificate: self.storage.arena[self.high_quorum_certificate]
                .quorum_certificate
                .clone()
                .unwrap(),
            signature: Signature::default(),
        };
        // self.on_propose_liveness(block_new);
        self.propose_parent = block_new;

        assert!(self.storage.arena[block_new].height > self.voted_height);
        self.do_broadcast_proposal(proposal, block_new);
        // self.on_receive_proposal(proposal, block_new);

        self.commit();
        block_new
    }

    fn on_receive_proposal(&mut self, proposal: Proposal, block_new: BlockId) {
        let self_propose = proposal.proposer == self.id;
        if !self_propose {
            // sanity check delivered
            assert_eq!(self.storage.arena[block_new].status, BlockStatus::Deciding);
            self.update(block_new);
        }
        let mut opinion = false;
        let arena = &self.storage.arena;
        if arena[block_new].height > self.voted_height {
            if arena[arena[block_new].certified].height > arena[self.block_lock].height {
                opinion = true;
                self.voted_height = arena[block_new].height;
            } else {
                let mut block = block_new;
                while arena[block].height > arena[self.block_lock].height {
                    block = arena[block].parent;
                }
                if block == self.block_lock {
                    opinion = true;
                    self.voted_height = arena[block_new].height;
                }
            }
        }
        // if !self_propose {
        //     self.on_quorum_certificate_finish(arena[block_new].quorum_certificate_reference);
        // }
        // self.on_receive_proposal_liveness(block_new);
        self.propose_parent = block_new;

        if opinion {
            let block_hash = self.storage.arena[block_new].hash;
            // println!("vote   {block_hash:02x?}");
            self.do_vote(
                proposal.proposer,
                Vote {
                    voter: self.id,
                    block_hash,
                    signature: Signature::default(),
                },
            );
        }
        self.commit();
    }

    fn on_receive_vote(&mut self, vote: Vote) {
        let block = self.storage.block_ids[&vote.block_hash];
        let arena = &mut self.storage.arena;
        let quorum_size = arena[block].voted.len();
        if quorum_size >= self.transport.config.n - self.transport.config.f {
            return;
        }
        if !arena[block].voted.insert(vote.voter) {
            println!(
                "! duplicate vote for {:02x?} from {}",
                vote.block_hash, vote.voter
            );
            return;
        }
        let quorum_certificate = arena[block].quorum_certificate.get_or_insert_with(|| {
            println!("! vote for block not proposed by itself");
            Vec::new()
        });
        quorum_certificate.push((vote.voter, vote.signature));
        if quorum_size + 1 == self.transport.config.n - self.transport.config.f {
            self.profile.quorum_certificate_finish.push(Instant::now());
            // compute
            self.update_high_quorum_certificate(block);
            self.propose_parent = block;

            // self.on_quorum_certificate_finish(block);
            self.quorum_certificate_finished = true;
            self.beat();
        }
    }

    fn add_block(&mut self, mut block: StorageBlock) -> BlockId {
        block.hash = digest((&block.requests, &block.parent_hash));
        let block_id = self.storage.arena.len();
        self.storage.block_ids.insert(block.hash, block_id);
        self.storage.arena.push(block);
        block_id
    }

    fn on_deliver_block(&mut self, block: BlockId) -> bool {
        let arena = &mut self.storage.arena;
        if arena[block].status != BlockStatus::Delivering {
            println!("! attempt to deliver a block twice");
            return false;
        }
        arena[block].parent = self.storage.block_ids[&arena[block].parent_hash];
        arena[block].height = arena[arena[block].parent].height + 1;

        arena[block].certified = self.storage.block_ids[&arena[block].certified_hash];

        // self.core.tails.remove(&arena[block].parent);
        // self.core.tails.insert(block);

        arena[block].status = BlockStatus::Deciding;
        true
    }

    fn is_block_delivered(&self, hash: &Digest) -> bool {
        if let Some(&block) = self.storage.block_ids.get(hash) {
            self.storage.arena[block].status != BlockStatus::Delivering
        } else {
            false
        }
    }

    fn get_proposer(&self) -> ReplicaId {
        0 // TODO rotate
    }

    fn get_parent(&self) -> BlockId {
        self.propose_parent
    }

    fn do_broadcast_proposal(&mut self, proposal: Proposal, block: BlockId) {
        // self.transport
        //     .send_message(ToAll, Message::Proposal(proposal));
        self.block_proposal = block;
        // self.transport
        //     .send_signed_message(ToSelf, Message::Proposal(proposal), self.id);
        self.transport
            .send_message(ToAll, Message::Proposal(proposal.clone()));
        self.profile.send_proposal.push(Instant::now());
        self.on_receive_proposal(proposal, self.block_proposal);
    }

    fn update_high_quorum_certificate(&mut self, high_quorum_certificate: BlockId) {
        let arena = &self.storage.arena;
        // assert_eq!(
        //     arena[high_quorum_certificate].hash,
        //     quorum_certificate.object_hash
        // );
        if arena[high_quorum_certificate].height > arena[self.high_quorum_certificate].height {
            self.high_quorum_certificate = high_quorum_certificate;
            // self.on_high_quorum_certificate_update(high_quorum_certificate);
        }
    }

    fn update(&mut self, new_block: BlockId) {
        let arena = &self.storage.arena;
        // println!("update {:02x?}", arena[new_block].hash);
        let block2 = arena[new_block].certified;
        if arena[block2].status == BlockStatus::Decided {
            return;
        }
        self.update_high_quorum_certificate(block2);

        let arena = &self.storage.arena;
        let block1 = arena[block2].certified;
        if arena[block1].status == BlockStatus::Decided {
            return;
        }
        if arena[block1].height > arena[self.block_lock].height {
            self.block_lock = block1;
        }

        let block = arena[block1].certified;
        if arena[block].status == BlockStatus::Decided {
            return;
        }

        if block1 != arena[block2].parent || block != arena[block1].parent {
            return;
        }

        // let mut commit_blocks = Vec::new();
        // let mut b = block;
        // while arena[b].height > arena[self.block_execute].height {
        //     commit_blocks.push(b);
        //     b = arena[b].parent;
        // }
        // assert_eq!(b, self.block_execute);
        // for block in commit_blocks.into_iter().rev() {
        //     // println!("commit {:02x?}", self.storage.arena[block].hash);
        //     self.storage.arena[block].status = BlockStatus::Decided;
        //     // self.do_consensus(block);
        //     for i in 0..self.storage.arena[block].requests.len() {
        //         self.do_decide(block, i);
        //     }
        // }
        // self.block_execute = block;
        self.block_commit = block;
    }

    fn commit(&mut self) {
        let mut commit_blocks = Vec::new();
        let mut b = self.block_commit;
        let arena = &self.storage.arena;
        while arena[b].height > arena[self.block_execute].height {
            commit_blocks.push(b);
            b = arena[b].parent;
        }
        assert_eq!(b, self.block_execute);
        for block in commit_blocks.into_iter().rev() {
            // println!("commit {:02x?}", self.storage.arena[block].hash);
            self.storage.arena[block].status = BlockStatus::Decided;
            // self.do_consensus(block);
            for i in 0..self.storage.arena[block].requests.len() {
                self.do_decide(block, i);
            }
        }
        self.block_execute = self.block_commit;
    }

    fn do_vote(&mut self, proposer: ReplicaId, vote: Vote) {
        // PaceMakerRR has a trivial `beat_resp` so simply inline here
        self.transport.send_signed_message(
            if proposer != self.id {
                ToReplica(proposer)
            } else {
                ToSelf
            },
            Message::Vote(vote),
            self.id,
        );
    }

    fn do_decide(&mut self, block: BlockId, i: usize) {
        let block = &self.storage.arena[block];
        let request = &block.requests[i];
        let result = self.app.replica_upcall(block.height, &request.op);
        let reply = Reply {
            request_number: request.request_number,
            result,
            replica_id: self.id,
            signature: Signature::default(),
        };
        self.client_table
            .insert_commit(request.client_id, request.request_number, reply.clone());
        // self.transport
        //     .send_signed_message(To(request.client_id.0), Message::Reply(reply), self.id);
        self.transport
            .send_message(To(request.client_id.0), Message::Reply(reply));
    }
}

impl Drop for Replica {
    fn drop(&mut self) {
        if self.id != self.get_proposer() {
            return;
        }

        let mut n_block = 0;
        let mut n_op = 0;
        for block in &self.storage.arena {
            if block.status != BlockStatus::Decided {
                continue;
            }
            n_block += 1;
            n_op += block.requests.len();
        }
        println!("average batch size {}", n_op as f32 / n_block as f32);

        // let mut instants = Vec::new();
        // instants.extend(
        //     self.profile
        //         .enter_request
        //         .drain(..)
        //         .map(|instant| (instant, 0)),
        // );
        // instants.extend(
        //     self.profile
        //         .exit_request
        //         .drain(..)
        //         .map(|instant| (instant, 1)),
        // );
        // instants.extend(
        //     self.profile
        //         .quorum_certificate_finish
        //         .drain(..)
        //         .map(|instant| (instant, 2)),
        // );
        // instants.extend(
        //     self.profile
        //         .send_proposal
        //         .drain(..)
        //         .map(|instant| (instant, 6)),
        // );

        // instants.sort_unstable();

        // let mut last_proposal = Instant::now();
        // let mut vote_delays = Vec::new();
        // let mut proposal_intervals = Vec::new();
        // let (mut last_instant, mut _last_id) = instants.first().unwrap();
        // let mut request_delays = Vec::new();
        // for (instant, id) in instants.into_iter() {
        //     if id == 6 {
        //         if instant > last_proposal {
        //             proposal_intervals.push(instant - last_proposal);
        //         }
        //         last_proposal = instant;
        //     }
        //     if id == 2 {
        //         if instant < last_proposal {
        //             println!("ignore early vote");
        //             continue;
        //         }
        //         vote_delays.push(instant - last_proposal);
        //     }
        //     if id == 0 && instant != last_instant {
        //         request_delays.push(instant - last_instant);
        //     }
        //     (last_instant, _last_id) = (instant, id);
        // }

        // println!("vote delay");
        // println!("{}", LatencyDistribution::from(vote_delays));
        // println!("proposal interval");
        // println!("{}", LatencyDistribution::from(proposal_intervals));
        // println!("request delay");
        // println!("{}", LatencyDistribution::from(request_delays));
    }
}

struct LatencyDistribution(Vec<Duration>);
impl From<Vec<Duration>> for LatencyDistribution {
    fn from(mut durations: Vec<Duration>) -> Self {
        durations.sort_unstable();
        Self(durations)
    }
}

impl Display for LatencyDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut range = Duration::ZERO..self.0[self.0.len() / 100];
        let mut range_total = Duration::ZERO;
        let mut range_count = 0;
        for &item in &self.0 {
            while item > range.end {
                if range_count != 0 {
                    writeln!(f, "{range:12?} {range_total:12?} {range_count:8}")?;
                    range_total = Duration::ZERO;
                    range_count = 0;
                }
                range = range.end..range.end * 2;
            }
            range_total += item;
            range_count += 1;
        }
        write!(f, "{range:12?} {range_total:12?} {range_count:8}")
    }
}
