use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{
    common::{ClientTable, Reorder},
    crypto::{CryptoMessage, Signature},
    meta::{
        digest, ClientId, Digest, OpNumber, ReplicaId, RequestNumber, ViewNumber, ENTRY_NUMBER,
    },
    transport::{
        Destination::{To, ToAll, ToReplica, ToSelf},
        InboundAction::{Allow, Block, VerifyReplica},
        InboundPacket::Unicast,
        Node, Transport, TransportMessage,
    },
    App, InvokeResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    Reply(Reply),
    PrePrepare(PrePrepare, Vec<Request>),
    Prepare(Prepare),
    Commit(Commit),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    client_id: ClientId,
    request_number: RequestNumber,
    op: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    view_number: ViewNumber,
    request_number: RequestNumber,
    client_id: ClientId,
    replica_id: ReplicaId,
    result: Vec<u8>,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrePrepare {
    view_number: ViewNumber,
    op_number: OpNumber,
    digest: Digest,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prepare {
    view_number: ViewNumber,
    op_number: OpNumber,
    digest: Digest,
    replica_id: ReplicaId,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    view_number: ViewNumber,
    op_number: OpNumber,
    digest: Digest,
    replica_id: ReplicaId,
    signature: Signature,
}

impl CryptoMessage for Message {
    fn signature_mut(&mut self) -> &mut Signature {
        match self {
            Self::Request(_) => unreachable!(),
            Self::Reply(Reply { signature, .. })
            | Self::PrePrepare(PrePrepare { signature, .. }, _)
            | Self::Prepare(Prepare { signature, .. })
            | Self::Commit(Commit { signature, .. }) => signature,
        }
    }
}

pub struct Client {
    transport: Transport<Self>,
    id: ClientId,
    request_number: RequestNumber,
    invoke: Option<Invoke>,
    view_number: ViewNumber,
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
            view_number: 0,
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
        let message = if let TransportMessage::Allowed(Message::Reply(message)) = message {
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

        // TODO byzantine on first reply
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
            assert!(message.view_number >= self.view_number);
            self.view_number = message.view_number;
            invoke.continuation.send(message.result).unwrap();
        }
    }
}

impl Client {
    fn send_request(&mut self) {
        let invoke = &self.invoke.as_ref().unwrap();
        self.transport.send_message(
            if invoke.timer_id != 0 {
                ToAll
            } else {
                ToReplica(self.transport.config.primary(self.view_number))
            },
            Message::Request(invoke.request.clone()),
        );
        let request_number = invoke.request.request_number;
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
    app: Box<dyn App + Send>,

    view_number: ViewNumber,
    op_number: OpNumber,
    commit_number: OpNumber,
    log: Vec<LogEntry>,
    client_table: ClientTable<Reply>,
    reorder_pre_prepare: Reorder<(PrePrepare, Vec<Request>)>,
    prepare_quorums: HashMap<(OpNumber, Digest), HashMap<ReplicaId, Prepare>>,
    commit_quorums: HashMap<(OpNumber, Digest), HashMap<ReplicaId, Commit>>,
    batch: Vec<Request>,
    enable_batching: bool,
}

#[derive(Serialize, Deserialize)]
struct LogEntry {
    status: LogStatus,
    view_number: ViewNumber,
    requests: Vec<Request>,
    pre_prepare: PrePrepare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum LogStatus {
    Preparing,
    Committing,
    Committed,
}

impl Replica {
    const MAX_BATCH: usize = 10;
    // the max gap between op_number (proposed sequence) and commit_number
    // (executed seqeuence), where a lot of reordering is allowed to happen
    // the number here is somewhat based on, the protocol can be intuitively
    // pipelined into 7 stages according to critical path's signing/verifying,
    // i.e. sign + verify pre-prepare, sign + verify prepare, sign + verify
    // commit, and sign reply.
    // evaluation also shows that core utilization approaches 100% with this
    // configuration
    const MAX_CONCURRENT: u32 = 7;

    pub fn new(
        transport: Transport<Self>,
        id: ReplicaId,
        app: impl App + Send + 'static,
        enable_batching: bool,
    ) -> Self {
        Self {
            transport,
            id,
            app: Box::new(app),
            view_number: 0,
            op_number: 0,
            commit_number: 0,
            log: Vec::with_capacity(ENTRY_NUMBER),
            client_table: ClientTable::default(),
            reorder_pre_prepare: Reorder::new(1),
            prepare_quorums: HashMap::new(),
            commit_quorums: HashMap::new(),
            batch: Vec::new(),
            enable_batching,
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
        packet: crate::transport::InboundPacket<'_, Self::Message>,
    ) -> crate::transport::InboundAction<Self::Message> {
        let message = if let Unicast { message } = packet {
            message
        } else {
            return Block;
        };
        match message {
            Message::Request(_) => Allow(message),
            Message::PrePrepare(PrePrepare { view_number, .. }, _) => {
                if view_number == self.view_number {
                    VerifyReplica(message, self.transport.config.primary(self.view_number))
                } else {
                    Block
                }
            }
            Message::Prepare(Prepare { replica_id, .. })
            | Message::Commit(Commit { replica_id, .. }) => VerifyReplica(message, replica_id),
            _ => Block,
        }
    }

    fn receive_message(&mut self, message: TransportMessage<Self::Message>) {
        match message {
            TransportMessage::Allowed(Message::Request(message)) => self.handle_request(message),
            TransportMessage::Signed(Message::PrePrepare(message, requests)) => {
                self.transport.send_message(
                    ToAll,
                    Message::PrePrepare(message.clone(), requests.clone()),
                );
                self.insert_pre_prepare(message, requests);
            }
            TransportMessage::Verified(Message::PrePrepare(message, requests)) => {
                self.handle_pre_prepare(message, requests)
            }
            // careful for reusing
            TransportMessage::Signed(Message::Prepare(message)) => {
                self.transport
                    .send_message(ToAll, Message::Prepare(message.clone()));
                self.handle_prepare(message);
            }
            TransportMessage::Verified(Message::Prepare(message)) => self.handle_prepare(message),
            TransportMessage::Signed(Message::Commit(message)) => {
                self.transport
                    .send_message(ToAll, Message::Commit(message.clone()));
                self.handle_commit(message);
            }
            TransportMessage::Verified(Message::Commit(message)) => self.handle_commit(message),
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

        if self.transport.config.primary(self.view_number) != self.id {
            todo!()
        }
        self.batch.push(message);

        // loop here?
        if !self.enable_batching || self.op_number < self.commit_number + Self::MAX_CONCURRENT {
            self.close_batch();
        }
    }

    fn close_batch(&mut self) {
        self.op_number += 1;
        // let batch = take(&mut self.batch);
        let batch = self
            .batch
            .drain(..usize::min(self.batch.len(), Self::MAX_BATCH))
            .collect::<Vec<_>>();
        let digest = digest(&batch);
        let pre_prepare = PrePrepare {
            view_number: self.view_number,
            op_number: self.op_number,
            digest,
            signature: Signature::default(),
        };
        self.transport.send_signed_message(
            ToSelf,
            Message::PrePrepare(pre_prepare, batch),
            self.id,
        );
    }

    fn handle_pre_prepare(&mut self, message: PrePrepare, requests: Vec<Request>) {
        if message.view_number != self.view_number {
            return;
        }
        if message.op_number <= self.op_number {
            return;
        }
        if digest(&requests) != message.digest {
            return;
        }
        self.insert_pre_prepare(message, requests);
    }

    fn insert_pre_prepare(&mut self, message: PrePrepare, requests: Vec<Request>) {
        let mut ordered = self
            .reorder_pre_prepare
            .insert_reorder(message.op_number, (message, requests));
        while let Some((message, requests)) = ordered {
            self.prepare(message, requests);
            ordered = self.reorder_pre_prepare.expect_next();
        }
    }

    fn prepare(&mut self, message: PrePrepare, requests: Vec<Request>) {
        assert_eq!(message.op_number, self.log.len() as OpNumber + 1);
        if self.id != self.transport.config.primary(self.view_number) {
            assert_eq!(message.op_number, self.op_number + 1);
            self.op_number = message.op_number;
            let prepare = Prepare {
                view_number: self.view_number,
                op_number: self.op_number,
                digest: message.digest,
                replica_id: self.id,
                signature: Signature::default(),
            };
            self.transport
                .send_signed_message(ToSelf, Message::Prepare(prepare), self.id);
        }
        let op_number = message.op_number;
        let digest = message.digest;
        self.log.push(LogEntry {
            status: LogStatus::Preparing,
            view_number: self.view_number,
            requests,
            pre_prepare: message,
        });
        if self
            .prepare_quorums
            .get(&(op_number, digest))
            .map(|quorum| quorum.len() >= self.transport.config.f * 2)
            .unwrap_or(false)
        {
            self.commit(op_number);
        }
    }

    fn handle_prepare(&mut self, message: Prepare) {
        if message.view_number != self.view_number {
            return;
        }
        let entry = if let Some(entry) = self.log.get(message.op_number as usize - 1) {
            entry
        } else {
            return;
        };

        if entry.status == LogStatus::Committing || entry.status == LogStatus::Committed {
            // reply for slow peer
            return;
        }

        let quorum = self
            .prepare_quorums
            .entry((message.op_number, message.digest))
            .or_default();
        quorum.insert(message.replica_id, message.clone());
        if quorum.len() == self.transport.config.f * 2 {
            self.commit(message.op_number);
        }
    }

    // in PBFT commit is entering commit phase
    // reaching commit point is `execute`
    fn commit(&mut self, op_number: OpNumber) {
        let entry = &mut self.log[op_number as usize - 1];
        assert_eq!(entry.status, LogStatus::Preparing);
        entry.status = LogStatus::Committing;
        let commit = Commit {
            view_number: self.view_number,
            op_number,
            digest: entry.pre_prepare.digest,
            replica_id: self.id,
            signature: Signature::default(),
        };
        self.transport
            .send_signed_message(ToSelf, Message::Commit(commit), self.id);
        // should be fine to not consider the case that commit certification is
        // already collected, because the `ToSelf` message above will bring us
        // into `handle_commit` at least once
    }

    fn handle_commit(&mut self, message: Commit) {
        if message.view_number != self.view_number {
            return;
        }
        let op_number = message.op_number;
        let status = if let Some(entry) = self.log.get(op_number as usize - 1) {
            entry.status
        } else {
            return;
        };
        if status == LogStatus::Committed {
            return;
        }

        let quorum = self
            .commit_quorums
            .entry((op_number, message.digest))
            .or_default();
        quorum.insert(message.replica_id, message);
        if status == LogStatus::Committing
            && quorum.len() >= self.transport.config.n - self.transport.config.f
        {
            self.execute(op_number);
        }
    }

    fn execute(&mut self, op_number: OpNumber) {
        let entry = &mut self.log[op_number as usize - 1];
        assert_eq!(entry.status, LogStatus::Committing);
        entry.status = LogStatus::Committed;

        while let Some(entry) = self.log.get(self.commit_number as usize) {
            if entry.status != LogStatus::Committed {
                break;
            }
            self.commit_number += 1;
            for request in &entry.requests {
                let result = self.app.replica_upcall(self.commit_number, &request.op);
                let reply = Reply {
                    view_number: self.view_number,
                    request_number: request.request_number,
                    client_id: request.client_id,
                    result,
                    replica_id: self.id,
                    signature: Signature::default(),
                };
                self.client_table.insert_commit(
                    request.client_id,
                    request.request_number,
                    reply.clone(),
                );
                // self.transport.send_signed_message(
                //     To(request.client_id.0),
                //     Message::Reply(reply),
                //     self.id,
                // );
                self.transport
                    .send_message(To(request.client_id.0), Message::Reply(reply));
            }
        }

        // adaptive batching
        if self.id == self.transport.config.primary(self.view_number) {
            while !self.batch.is_empty()
                && self.op_number < self.commit_number + Self::MAX_CONCURRENT
            {
                self.close_batch();
            }
        }
    }
}

impl Drop for Replica {
    fn drop(&mut self) {
        if self.op_number != 0 {
            let n_request = self
                .log
                .iter()
                .map(|entry| entry.requests.len())
                .sum::<usize>();
            println!(
                "Average batch size {}",
                n_request as f32 / self.op_number as f32
            );
        }
    }
}
