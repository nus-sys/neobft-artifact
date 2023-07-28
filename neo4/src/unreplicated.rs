use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{
    common::ClientTable,
    crypto::{CryptoMessage, Signature},
    meta::{ClientId, OpNumber, ReplicaId, RequestNumber, ENTRY_NUMBER},
    transport::{
        Destination::{To, ToReplica},
        Node, Transport, TransportMessage,
    },
    App, InvokeResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    client_id: ClientId,
    request_number: RequestNumber,
    op: Vec<u8>,
}

impl CryptoMessage for Request {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    request_number: RequestNumber,
    result: Vec<u8>,
    signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    Reply(Reply),
}

impl CryptoMessage for Message {
    fn signature_mut(&mut self) -> &mut Signature {
        match self {
            Self::Request(_) => unreachable!(),
            Self::Reply(Reply { signature, .. }) => signature,
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
        if self.invoke.is_none() {
            return;
        }
        if message.request_number != self.invoke.as_ref().unwrap().request.request_number {
            return;
        }
        let invoke = self.invoke.take().unwrap();
        self.transport.cancel_timer(invoke.timer_id);
        invoke.continuation.send(message.result).unwrap();
    }
}

impl Client {
    fn send_request(&mut self) {
        let request = &self.invoke.as_ref().unwrap().request;
        self.transport
            .send_message(ToReplica(0), Message::Request(request.clone()));
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
            .create_timer(Duration::from_millis(100), on_resend);
    }
}

pub struct Replica {
    transport: Transport<Self>,
    // id: ReplicaId,
    op_number: OpNumber,
    app: Box<dyn App + Send>,
    client_table: ClientTable<Reply>,
    log: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntry {
    request: Request,
}

impl Replica {
    pub fn new(transport: Transport<Self>, id: ReplicaId, app: impl App + Send + 'static) -> Self {
        assert_eq!(id, 0);
        Self {
            transport,
            // id,
            op_number: 0,
            app: Box::new(app),
            client_table: ClientTable::default(),
            log: Vec::with_capacity(ENTRY_NUMBER),
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

    fn receive_message(&mut self, message: TransportMessage<Self::Message>) {
        let message = if let TransportMessage::Allowed(Message::Request(message)) = message {
            message
        } else {
            unreachable!()
        };
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

        self.op_number += 1;
        let result = self.app.replica_upcall(self.op_number, &message.op);
        let reply = Reply {
            request_number: message.request_number,
            result,
            signature: Signature::default(),
        };
        self.log.push(LogEntry {
            request: message.clone(),
        });
        assert_eq!(self.log.len() as OpNumber, self.op_number);
        self.client_table
            .insert_commit(message.client_id, message.request_number, reply.clone());
        // self.transport
        //     .send_signed_message(To(message.client_id.0), Message::Reply(reply), self.id);
        self.transport
            .send_message(To(message.client_id.0), Message::Reply(reply));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{task::yield_now, time::timeout};

    use crate::{
        common::TestApp,
        crypto::Executor,
        transport::{
            simulated::{BasicSwitch, Network},
            Concurrent, Run, Transport,
        },
        Client as _,
    };

    use super::{Client, Replica};

    #[tokio::test(start_paused = true)]
    async fn single_op() {
        let config = Network::config(1, 0);
        let mut net = Network(BasicSwitch::default());
        let replica = Replica::new(
            Transport::new(
                config.clone(),
                net.insert_socket(config.replicas[0]),
                Executor::Inline,
            ),
            0,
            TestApp::default(),
        );
        let mut client = Client::new(Transport::new(
            config.clone(),
            net.insert_socket(Network::client(0)),
            Executor::Inline,
        ));

        let net = Concurrent::run(net);
        yield_now().await;
        let replica = Concurrent::run(replica);
        let result = client.invoke("hello".as_bytes());
        timeout(
            Duration::from_millis(20),
            client.run(async {
                assert_eq!(&result.await, "[1] hello".as_bytes());
            }),
        )
        .await
        .unwrap();

        let replica = replica.join().await;
        net.join().await;
        assert_eq!(replica.log.len(), 1);
    }
}
