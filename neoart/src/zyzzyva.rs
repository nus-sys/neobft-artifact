use std::{
    collections::{HashMap, HashSet},
    mem::take,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{
    common::{ClientTable, Reorder},
    crypto::{verify_message, CryptoMessage, Signature},
    meta::{
        digest, ClientId, Config, Digest, OpNumber, ReplicaId, RequestNumber, ViewNumber,
        ENTRY_NUMBER,
    },
    transport::{
        Destination::{To, ToAll, ToReplica, ToSelf},
        InboundAction, InboundPacket, Node, Transport, TransportMessage,
    },
    App, InvokeResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    OrderReq(OrderReq, Vec<Request>),
    SpecResponse(SpecResponse, ReplicaId, Vec<u8>, OrderReq),
    Commit(Commit),
    LocalCommit(LocalCommit),
    Checkpoint(Checkpoint),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    client_id: ClientId,
    request_number: RequestNumber,
    op: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrderReq {
    view_number: ViewNumber,
    op_number: OpNumber,
    history_digest: Digest,
    message_digest: Digest,
    // nondeterministic: (),
    signature: Signature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SpecResponse {
    view_number: ViewNumber,
    op_number: OpNumber,
    history_digest: Digest,
    result_digest: Digest,
    client_id: ClientId,
    request_number: RequestNumber,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    client_id: ClientId,
    certificate: CommitCertificate,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CommitCertificate {
    spec_response: SpecResponse, // signature cleared
    signatures: Vec<(ReplicaId, Signature)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalCommit {
    view_number: ViewNumber,
    message_digest: Digest,
    history_digest: Digest,
    replica_id: ReplicaId,
    client_id: ClientId,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    op_number: OpNumber,
    history_digest: Digest,
    // app_snapshot: (),
    replica_id: ReplicaId,
    signature: Signature,
}

impl CryptoMessage for Message {
    fn signature_mut(&mut self) -> &mut Signature {
        match self {
            Self::OrderReq(OrderReq { signature, .. }, _)
            | Self::SpecResponse(SpecResponse { signature, .. }, ..)
            | Self::LocalCommit(LocalCommit { signature, .. })
            | Self::Checkpoint(Checkpoint { signature, .. }) => signature,
            _ => unreachable!(),
        }
    }

    fn digest(&self) -> Digest {
        match self {
            // according to paper's specification the digest of messages below
            // does not cover the rest part of the messages
            // not a big deal just follow the specification as good as we can
            Self::OrderReq(message, ..) => digest(message),
            Self::SpecResponse(message, ..) => digest(message),
            _ => digest(self),
        }
    }
}

impl Message {
    fn verify_commit(&mut self, config: &Config) -> bool {
        let certificate = if let Self::Commit(message) = self {
            &message.certificate
        } else {
            unreachable!()
        };
        if certificate.signatures.len() < config.f * 2 + 1 {
            return false;
        }
        for &(replica_id, signature) in &certificate.signatures {
            let mut spec_response = Message::SpecResponse(
                SpecResponse {
                    signature,
                    ..certificate.spec_response
                },
                // the rest parts' content does not really matter since
                // `signature` is not covering them
                replica_id,
                Vec::default(),
                OrderReq::default(),
            );
            if !verify_message(
                &mut spec_response,
                &config.keys[replica_id as usize].public_key(),
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
    view_number: ViewNumber,
    assume_byz: bool,
}

struct Invoke {
    request: Request,
    certificate: CommitCertificate,
    result: Vec<u8>,
    local_committed: HashSet<ReplicaId>,
    continuation: oneshot::Sender<Vec<u8>>,
    timer_id: u32,
}

impl Client {
    pub fn new(transport: Transport<Self>, assume_byz: bool) -> Self {
        Self {
            id: transport.create_id(),
            transport,
            request_number: 0,
            invoke: None,
            view_number: 0,
            assume_byz,
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
            certificate: CommitCertificate::default(),
            local_committed: HashSet::new(),
            result: Vec::new(),
        });
        self.send_request();
        Box::pin(async { result.await.unwrap() })
    }
}

impl Node for Client {
    type Message = Message;

    fn receive_message(&mut self, message: TransportMessage<Self::Message>) {
        match message {
            TransportMessage::Allowed(Message::SpecResponse(message, replica_id, result, _)) => {
                self.handle_spec_response(message, replica_id, result)
            }
            TransportMessage::Allowed(Message::LocalCommit(message)) => {
                self.handle_local_commit(message)
            }
            _ => unreachable!(),
        }
    }
}

impl Client {
    fn handle_spec_response(
        &mut self,
        mut message: SpecResponse,
        replica_id: ReplicaId,
        result: Vec<u8>,
    ) {
        if message.client_id != self.id || message.request_number != self.request_number {
            return;
        }
        let invoke = if let Some(invoke) = self.invoke.as_mut() {
            invoke
        } else {
            return;
        };

        let signature = take(&mut message.signature);
        if invoke.certificate.signatures.is_empty() {
            invoke.certificate.spec_response = message;
            invoke.result = result;
        } else if message != invoke.certificate.spec_response || result != invoke.result {
            println!(
                "! client {} mismatched result request {} replica {}",
                self.id, message.request_number, replica_id
            );
            return;
        }
        if invoke
            .certificate
            .signatures
            .iter()
            .all(|&(id, _)| replica_id != id)
        {
            invoke.certificate.signatures.push((replica_id, signature));
        }

        if self.assume_byz && invoke.certificate.signatures.len() == self.transport.config.f * 2 + 1
        {
            self.transport.cancel_timer(invoke.timer_id);
            self.send_request();
        } else if invoke.certificate.signatures.len() == self.transport.config.n {
            let invoke = self.invoke.take().unwrap();
            self.transport.cancel_timer(invoke.timer_id);
            self.view_number = invoke.certificate.spec_response.view_number;
            invoke.continuation.send(invoke.result).unwrap();
        }
    }

    fn handle_local_commit(&mut self, message: LocalCommit) {
        if message.client_id != self.id {
            return;
        }
        let invoke = if let Some(invoke) = self.invoke.as_mut() {
            invoke
        } else {
            return;
        };

        invoke.local_committed.insert(message.replica_id);
        if invoke.local_committed.len() == self.transport.config.f * 2 + 1 {
            let invoke = self.invoke.take().unwrap();
            self.transport.cancel_timer(invoke.timer_id);
            self.view_number = message.view_number; // any possible to go backward?
            invoke.continuation.send(invoke.result).unwrap();
        }
    }

    fn send_request(&mut self) {
        let invoke = self.invoke.as_mut().unwrap();
        if invoke.certificate.signatures.len() < self.transport.config.f * 2 + 1 {
            // timer not set already => this is not a resending
            let destination = if invoke.timer_id == 0 {
                ToReplica(self.transport.config.primary(self.view_number))
            } else {
                ToAll
            };
            self.transport
                .send_message(destination, Message::Request(invoke.request.clone()));
        } else {
            let commit = Commit {
                client_id: self.id,
                certificate: CommitCertificate {
                    spec_response: invoke.certificate.spec_response.clone(),
                    signatures: invoke.certificate.signatures[..self.transport.config.f * 2 + 1]
                        .to_vec(),
                },
            };
            self.transport.send_message(ToAll, Message::Commit(commit));
        }

        let request_number = self.request_number;
        let on_resend = move |receiver: &mut Self| {
            assert_eq!(
                receiver.invoke.as_ref().unwrap().request.request_number,
                request_number
            );
            println!("! client {} resend request {}", receiver.id, request_number);
            receiver.send_request();
        };
        invoke.timer_id = self
            .transport
            .create_timer(Duration::from_secs(1), on_resend);
    }
}

pub struct Replica {
    transport: Transport<Self>,
    id: ReplicaId,
    view_number: ViewNumber,
    // the #op that should be speculative committed up to, and the corresponded
    // history hash
    // on primary replica this pair is aligned with latest proposed request,
    // while on backup replicas this pair is aligned with latest received &
    // ordered OrderReq
    // well, maybe not a good idea to use states like this
    op_number: OpNumber,
    history_digest: Digest,

    app: Box<dyn App + Send>,
    // always Message::SpecResponse(..), i don't want to keep separated parts
    client_table: ClientTable<Message>,
    log: Vec<LogEntry>,
    commit_certificate: CommitCertificate, // highest
    reorder_order_req: Reorder<(OrderReq, Vec<Request>)>,
    batch_size: usize,
    batch: Vec<Request>,
    checkpoint_quorums: HashMap<(OpNumber, Digest), HashSet<ReplicaId>>,
    checkpoint_number: OpNumber,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntry {
    view_number: ViewNumber,
    requests: Vec<Request>,
    // spec_response: SignedMessage,
    message_digest: Digest,
    history_digest: Digest,
}

impl Replica {
    pub fn new(
        transport: Transport<Self>,
        id: ReplicaId,
        app: impl App + Send + 'static,
        batch_size: usize,
    ) -> Self {
        Self {
            transport,
            id,
            view_number: 0,
            op_number: 0,
            history_digest: Digest::default(),
            app: Box::new(app),
            client_table: ClientTable::default(),
            log: Vec::with_capacity(ENTRY_NUMBER),
            reorder_order_req: Reorder::new(1),
            commit_certificate: CommitCertificate::default(),
            batch_size,
            batch: Vec::new(),
            checkpoint_quorums: HashMap::new(),
            checkpoint_number: 0,
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
        buffer: InboundPacket<'_, Self::Message>,
    ) -> InboundAction<Self::Message> {
        let message = if let InboundPacket::Unicast { message, .. } = buffer {
            message
        } else {
            return InboundAction::Block;
        };
        match message {
            Message::Request(_) => InboundAction::Allow(message),
            Message::OrderReq(OrderReq { view_number, .. }, _) => {
                InboundAction::VerifyReplica(message, self.transport.config.primary(view_number))
            }
            // reduce duplicated verification?
            Message::Commit(_) => InboundAction::Verify(message, Message::verify_commit),
            _ => {
                println!("! [{}] unexpected {message:?}", self.id);
                InboundAction::Block
            }
        }
    }

    fn receive_message(&mut self, message: TransportMessage<Self::Message>) {
        match message {
            TransportMessage::Allowed(Message::Request(message)) => self.handle_request(message),
            TransportMessage::Verified(Message::OrderReq(message, request)) => {
                self.handle_order_req(message, request)
            }
            TransportMessage::Signed(Message::OrderReq(message, request)) => {
                // check view number?
                self.transport
                    .send_message(ToAll, Message::OrderReq(message.clone(), request.clone()));
                self.insert_order_req(message, request);
            }
            TransportMessage::Verified(Message::Commit(message)) => self.handle_commit(message),
            TransportMessage::Verified(Message::Checkpoint(message)) => {
                self.handle_checkpoint(message)
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
            resend(|response| {
                self.transport
                    .send_signed_message(To(message.client_id.0), response, self.id)
            });
            return;
        }

        if self.transport.config.primary(self.view_number) != self.id {
            todo!()
        }

        self.batch.push(message);
        if self.batch.len() < self.batch_size {
            return;
        }

        self.op_number += 1;
        let batch = take(&mut self.batch);
        let message_digest = digest(&batch);
        self.history_digest = digest([self.history_digest, message_digest]);
        let order_req = OrderReq {
            view_number: self.view_number,
            op_number: self.op_number,
            message_digest,
            history_digest: self.history_digest,
            signature: Signature::default(),
        };
        self.transport
            .send_signed_message(ToSelf, Message::OrderReq(order_req, batch), self.id);
    }

    fn handle_order_req(&mut self, message: OrderReq, request: Vec<Request>) {
        if message.view_number < self.view_number {
            return;
        }
        if message.view_number > self.view_number {
            todo!()
        }
        let is_primary = self.id == self.transport.config.primary(self.view_number);
        if !is_primary && message.message_digest != digest(&request) {
            println!(
                "! OrderReq incorrect message digest op {}",
                message.op_number
            );
            return;
        }

        self.insert_order_req(message, request);
    }

    fn insert_order_req(&mut self, message: OrderReq, request: Vec<Request>) {
        let mut ordered = self
            .reorder_order_req
            .insert_reorder(message.op_number, (message, request));
        while let Some((message, request)) = ordered {
            if self.id != self.transport.config.primary(self.view_number) {
                let previous_digest = if self.op_number == 0 {
                    Digest::default()
                } else {
                    self.log[(self.op_number - 1) as usize].history_digest
                };
                // revise this
                if message.history_digest != digest([previous_digest, message.message_digest]) {
                    println!(
                        "! OrderReq mismatched history digest op {}",
                        message.op_number
                    );
                    break;
                }

                // only update for backups here, primary updates when handle
                // requests (again, reusing state may not be good idea...)
                self.op_number += 1;
                self.history_digest = message.history_digest;
            }

            self.speculative_commit(message, request);
            ordered = self.reorder_order_req.expect_next();
        }
    }

    fn speculative_commit(&mut self, message: OrderReq, batch: Vec<Request>) {
        assert_eq!(message.view_number, self.view_number);

        for request in &batch {
            let result = self.app.replica_upcall(message.op_number, &request.op);
            let spec_response = Message::SpecResponse(
                SpecResponse {
                    view_number: self.view_number,
                    op_number: message.op_number,
                    result_digest: digest(&result),
                    history_digest: message.history_digest,
                    client_id: request.client_id,
                    request_number: request.request_number,
                    signature: Signature::default(),
                },
                self.id,
                result,
                message.clone(),
            );
            let client_id = request.client_id;
            let request_number = request.request_number;
            // is this SpecResponse always up to date?
            self.client_table
                .insert_commit(client_id, request_number, spec_response.clone());
            self.transport
                .send_signed_message(To(client_id.0), spec_response, self.id);
        }

        self.log.push(LogEntry {
            view_number: self.view_number,
            requests: batch,
            // spec_response,
            message_digest: message.message_digest,
            history_digest: message.history_digest,
        });
    }

    fn handle_commit(&mut self, message: Commit) {
        let spec_response = &message.certificate.spec_response;
        if self.view_number > spec_response.view_number {
            return;
        }
        if self.view_number < spec_response.view_number {
            todo!()
        }

        let entry = if let Some(entry) = self.log.get_mut((spec_response.op_number - 1) as usize) {
            entry
        } else {
            todo!()
        };
        if spec_response.history_digest != entry.history_digest {
            todo!()
        }

        if spec_response.op_number > self.commit_certificate.spec_response.op_number {
            self.commit_certificate = message.certificate;
        }
        let local_commit = Message::LocalCommit(LocalCommit {
            view_number: self.view_number,
            message_digest: entry.message_digest,
            history_digest: entry.history_digest,
            replica_id: self.id,
            client_id: message.client_id,
            signature: Signature::default(),
        });
        // self.transport
        //     .send_signed_message(To(message.client_id.0), local_commit, self.id);
        self.transport
            .send_message(To(message.client_id.0), local_commit);
    }

    fn handle_checkpoint(&mut self, message: Checkpoint) {
        if message.op_number <= self.checkpoint_number {
            return;
        }
        if let Some(entry) = self.log.get((message.op_number - 1) as usize) {
            if entry.history_digest != message.history_digest {
                return;
            }
        }

        let quorum = self
            .checkpoint_quorums
            .entry((message.op_number, message.history_digest))
            .or_default();
        quorum.insert(message.replica_id);
        if quorum.len() == self.transport.config.f + 1 {
            self.checkpoint_number = message.op_number;
            self.app.commit_upcall(self.checkpoint_number);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{str::from_utf8, time::Duration};

    use tokio::{
        spawn,
        task::{yield_now, JoinHandle},
        time::{sleep, timeout},
    };

    use crate::{
        common::TestApp,
        crypto::Executor,
        meta::ReplicaId,
        transport::{simulated::Network, Concurrent, Run, Transport},
        zyzzyva::{Client, Replica},
        Client as _,
    };

    struct System {
        net: Concurrent<Network>,
        replicas: Vec<Concurrent<Replica>>,
        clients: Vec<Client>,
    }

    impl System {
        async fn new(num_client: usize, assume_byz: bool, batch_size: usize) -> Self {
            let config = Network::config(4, 1);
            let mut net = Network::default();
            let clients = (0..num_client)
                .map(|i| {
                    Client::new(
                        Transport::new(
                            config.clone(),
                            net.insert_socket(Network::client(i)),
                            Executor::Inline,
                        ),
                        assume_byz,
                    )
                })
                .collect::<Vec<_>>();
            let replicas = (0..4)
                .map(|i| {
                    let replica = Replica::new(
                        Transport::new(
                            config.clone(),
                            net.insert_socket(config.replicas[i]),
                            Executor::Inline,
                        ),
                        i as ReplicaId,
                        TestApp::default(),
                        batch_size,
                    );
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
        let mut system = System::new(1, false, 1).await;
        let result = system.clients[0].invoke("hello".as_bytes());
        timeout(
            Duration::from_millis(30),
            system.clients[0].run(async {
                assert_eq!(&result.await, "[1] hello".as_bytes());
            }),
        )
        .await
        .unwrap();

        for replica in system.replicas {
            assert_eq!(replica.join().await.log.len(), 1);
        }
        system.net.join().await;
    }

    fn closed_loop(index: usize, mut client: Client) -> JoinHandle<()> {
        spawn(async move {
            for i in 0.. {
                let result = client.invoke(format!("op{index}-{i}").as_bytes());
                client
                    .run(async move {
                        let result = result.await;
                        assert!(
                            result
                                .strip_suffix(format!("op{index}-{i}").as_bytes())
                                .is_some(),
                            "expect op{index}-{i} get {}",
                            from_utf8(&result).unwrap()
                        );
                    })
                    .await;
            }
        })
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_closed_loop() {
        let num_client = 10;
        let mut system = System::new(num_client, false, 1).await;
        for (index, client) in system.clients.into_iter().enumerate() {
            closed_loop(index, client);
        }
        sleep(Duration::from_secs(1)).await;
        let primary_len = system.replicas.remove(0).join().await.log.len();
        assert!(primary_len >= 1000 / 30 * num_client);
        for replica in system.replicas {
            let backup_len = replica.join().await.log.len();
            // stronger assertions?
            assert!(backup_len <= primary_len);
            assert!(backup_len >= primary_len - num_client);
        }
        system.net.join().await;
    }

    #[tokio::test(start_paused = true)]
    async fn single_op_byzantine() {
        let mut system = System::new(1, true, 1).await;
        let _byz_replica = system.replicas.remove(3).join().await;
        let result = system.clients[0].invoke("hello".as_bytes());
        timeout(
            Duration::from_millis(50),
            system.clients[0].run(async {
                assert_eq!(&result.await, "[1] hello".as_bytes());
            }),
        )
        .await
        .unwrap();

        for replica in system.replicas {
            assert_eq!(replica.join().await.log.len(), 1);
        }
        system.net.join().await;
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_closed_loop_byzantine() {
        let num_client = 10;
        let mut system = System::new(num_client, true, 1).await;
        let _byz_replica = system.replicas.remove(3).join().await;
        for (index, client) in system.clients.into_iter().enumerate() {
            closed_loop(index, client);
        }
        sleep(Duration::from_secs(1)).await;
        let primary_len = system.replicas.remove(0).join().await.log.len();
        assert!(primary_len >= 1000 / 50 * num_client);
        for replica in system.replicas {
            let backup_len = replica.join().await.log.len();
            // stronger assertions?
            assert!(backup_len <= primary_len);
            assert!(backup_len >= primary_len - num_client);
        }
        system.net.join().await;
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_closed_loop_batching() {
        const BATCH_SIZE: usize = 10;
        let num_client = BATCH_SIZE * 4;
        let mut system = System::new(num_client, false, BATCH_SIZE).await;
        for (index, client) in system.clients.into_iter().enumerate() {
            closed_loop(index, client);
        }
        sleep(Duration::from_secs(1) - Duration::from_millis(1)).await;
        let primary_len = system.replicas.remove(0).join().await.log.len();
        assert!(primary_len >= 1000 / 30 * (num_client / BATCH_SIZE));
        for replica in system.replicas {
            let backup_len = replica.join().await.log.len();
            // stronger assertions?
            assert!(backup_len <= primary_len);
            assert!(backup_len >= primary_len - num_client / BATCH_SIZE);
        }
        system.net.join().await;
    }
}
