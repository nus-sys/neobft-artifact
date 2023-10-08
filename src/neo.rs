use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{
    client::BoxedConsume,
    common::{Request, Timer},
    context::{
        crypto::{DigestHash, Sign, Signed, Verify},
        ordered_multicast::OrderedMulticast,
        ClientIndex, Host, OrderedMulticastReceivers, Receivers, ReplicaIndex, To,
    },
    App, Context,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(OrderedMulticast<Request>),
    Reply(Signed<Reply>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    request_num: u32,
    result: Vec<u8>,
    epoch_num: u32,
    seq_num: u32,
    replica_index: ReplicaIndex,
}

#[derive(Debug)]
pub struct Client {
    index: ClientIndex,
    shared: Arc<Mutex<ClientShared>>,
}

#[derive(Debug)]
struct ClientShared {
    context: Context<Message>,
    request_num: u32,
    invoke: Option<ClientInvoke>,
    resend_timer: Timer,
}

#[derive(Debug)]
struct ClientInvoke {
    op: Vec<u8>,
    replies: HashMap<ReplicaIndex, Reply>,
    consume: BoxedConsume,
}

impl Client {
    pub fn new(context: Context<Message>, index: ClientIndex) -> Self {
        Self {
            index,
            shared: Arc::new(Mutex::new(ClientShared {
                context,
                request_num: 0,
                invoke: None,
                resend_timer: Timer::new(Duration::from_millis(100)),
            })),
        }
    }
}

impl crate::Client for Client {
    type Message = Message;

    fn invoke(&self, op: Vec<u8>, consume: impl Into<BoxedConsume>) {
        let shared = &mut *self.shared.lock().unwrap();
        assert!(shared.invoke.is_none());
        shared.request_num += 1;
        shared.invoke = Some(ClientInvoke {
            op: op.clone(),
            replies: Default::default(),
            consume: consume.into(),
        });
        let request = Request {
            client_index: self.index,
            request_num: shared.request_num,
            op,
        };
        shared.context.send_ordered_multicast(request);
        shared.resend_timer.set(&mut shared.context)
    }

    fn handle(&self, message: Self::Message) {
        let Message::Reply(reply) = message else {
            unimplemented!()
        };
        let shared = &mut *self.shared.lock().unwrap();
        if reply.request_num != shared.request_num {
            return;
        }
        let Some(invoke) = &mut shared.invoke else {
            return;
        };
        invoke
            .replies
            .insert(reply.replica_index, Reply::clone(&reply));
        let incoming_reply = reply;
        if invoke
            .replies
            .values()
            .filter(|reply| {
                (
                    reply.epoch_num, //
                    reply.seq_num,
                    &reply.result,
                ) == (
                    incoming_reply.epoch_num,
                    incoming_reply.seq_num,
                    &incoming_reply.result,
                )
            })
            .count()
            >= shared.context.config().num_replica - shared.context.config().num_faulty
        {
            shared.resend_timer.unset(&mut shared.context);
            let invoke = shared.invoke.take().unwrap();
            let _op = invoke.op;
            invoke.consume.apply(incoming_reply.inner.result)
        }
    }
}

#[derive(Debug)]
pub struct Replica {
    context: Context<Message>,
    index: ReplicaIndex,

    seq_num_offset: Option<u32>,
    reordering_requests: HashMap<u32, OrderedMulticast<Request>>,
    requests: Vec<OrderedMulticast<Request>>,
    replies: HashMap<ClientIndex, Reply>,
    app: App,
}

impl Replica {
    pub fn new(context: Context<Message>, index: ReplicaIndex, app: App) -> Self {
        Self {
            context,
            index,
            seq_num_offset: None,
            reordering_requests: Default::default(),
            requests: Default::default(),
            replies: Default::default(),
            app,
        }
    }
}

impl Receivers for Replica {
    type Message = Message;

    fn handle(&mut self, _: Host, _: Host, message: Self::Message) {
        let Message::Request(request) = message else {
            unimplemented!()
        };
        // Jialin's trick to avoid resetting switch for every run
        let op_num = request.seq_num - *self.seq_num_offset.get_or_insert(request.seq_num) + 1;
        let next_op_num = (self.requests.len() + 1) as _;
        assert!(op_num >= next_op_num);
        if op_num != next_op_num {
            self.reordering_requests.insert(op_num, request);
            assert!(self.reordering_requests.len() < 100);
            return;
        }
        self.do_commit(request);
        while let Some(request) = {
            let next_op_num = (self.requests.len() - 1) as _;
            self.reordering_requests.remove(&next_op_num)
        } {
            self.do_commit(request)
        }
    }

    fn on_timer(&mut self, _: Host, _: crate::context::TimerId) {
        unreachable!()
    }
}

impl OrderedMulticastReceivers for Replica {
    type Message = Request;
}

impl Replica {
    fn do_commit(&mut self, request: OrderedMulticast<Request>) {
        self.requests.push(request);
        let request = self.requests.last().unwrap();
        match self.replies.get(&request.client_index) {
            Some(reply) if reply.request_num > request.request_num => return,
            Some(reply) if reply.request_num == request.request_num => {
                self.context
                    .send(To::client(request.client_index), reply.clone());
                return;
            }
            _ => {}
        }
        let reply = Reply {
            epoch_num: 0,
            request_num: request.request_num,
            result: self.app.execute(&request.op),
            seq_num: request.seq_num,
            replica_index: self.index,
        };
        self.context.send(To::client(request.client_index), reply)
    }
}

impl From<OrderedMulticast<Request>> for Message {
    fn from(value: OrderedMulticast<Request>) -> Self {
        Self::Request(value)
    }
}

impl DigestHash for Reply {
    fn hash(&self, hasher: &mut impl std::hash::Hasher) {
        hasher.write_u32(self.request_num);
        hasher.write(&self.result);
        hasher.write_u32(self.epoch_num);
        hasher.write_u32(self.seq_num);
        hasher.write_u8(self.replica_index)
    }
}

impl Sign<Reply> for Message {
    fn sign(message: Reply, signer: &crate::context::crypto::Signer) -> Self {
        Message::Reply(signer.sign_private(message))
    }
}

impl Verify for Message {
    fn verify(
        &self,
        verifier: &crate::context::crypto::Verifier,
    ) -> Result<(), crate::context::crypto::Invalid> {
        match self {
            Self::Request(message) => verifier.verify_ordered_multicast(message),
            Self::Reply(message) => verifier.verify(message, message.replica_index),
        }
    }
}
