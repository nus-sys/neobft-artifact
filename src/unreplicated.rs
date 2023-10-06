use std::{collections::HashMap, sync::Mutex, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    client::OnResult,
    common::Timer,
    context::{ClientIndex, Context, DigestHash, Receivers, To},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    Reply(Reply),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    client_index: ClientIndex,
    request_num: u32,
    op: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    request_num: u32,
    result: Vec<u8>,
}

pub struct Client {
    index: ClientIndex,
    shared: Mutex<ClientShared>,
}

struct ClientShared {
    context: Context<Message>,
    request_num: u32,
    op: Option<Vec<u8>>,
    on_result: Option<Box<dyn OnResult + Send + Sync>>,
    resend_timer: Timer,
}

impl Client {
    pub fn new(context: Context<Message>, index: ClientIndex) -> Self {
        Self {
            index,
            shared: Mutex::new(ClientShared {
                context,
                request_num: 0,
                op: None,
                on_result: None,
                resend_timer: Timer::new(Duration::from_millis(100)),
            }),
        }
    }
}

impl crate::Client for Client {
    type Message = Message;

    fn invoke(&self, op: Vec<u8>, on_result: impl Into<Box<dyn OnResult + Send + Sync>>) {
        let shared = &mut *self.shared.lock().unwrap();
        shared.request_num += 1;
        assert!(shared.op.is_none());
        shared.op = Some(op.clone());
        shared.on_result = Some(on_result.into());
        shared.resend_timer.set(&mut shared.context);

        let request = Request {
            client_index: self.index,
            request_num: shared.request_num,
            op,
        };
        shared
            .context
            .send(To::Replica(0), Message::Request(request))
    }

    fn handle(&self, message: Self::Message) {
        let Message::Reply(reply) = message else {
            unimplemented!()
        };
        let shared = &mut *self.shared.lock().unwrap();
        if reply.request_num != shared.request_num {
            return;
        }
        shared.op.take().unwrap();
        shared.resend_timer.unset(&mut shared.context);
        shared.on_result.take().unwrap().apply(reply.result);
    }
}

pub struct Replica {
    context: Context<Message>,
    replies: HashMap<ClientIndex, Reply>,
    app: (),
}

impl Replica {
    pub fn new(context: Context<Message>) -> Self {
        Self {
            context,
            replies: Default::default(),
            app: (),
        }
    }
}

impl Receivers for Replica {
    type Message = Message;

    fn handle(&mut self, to: To, remote: To, message: Self::Message) {
        assert_eq!(to, To::Replica(0));
        let Message::Request(request) = message else {
            unimplemented!()
        };
        let To::Client(index) = remote else {
            unimplemented!()
        };
        match self.replies.get(&index) {
            Some(reply) if reply.request_num > request.request_num => return,
            Some(reply) if reply.request_num == request.request_num => {
                self.context.send(remote, Message::Reply(reply.clone()));
                return;
            }
            _ => {}
        }
        let reply = Reply {
            request_num: request.request_num,
            result: Default::default(), // TODO
        };
        self.replies.insert(index, reply.clone());
        self.context.send(remote, Message::Reply(reply))
    }

    fn on_timer(&mut self, _: To, _: crate::context::TimerId) {
        unreachable!()
    }
}

impl DigestHash for Message {
    fn hash(&self, hasher: &mut crate::context::Hasher) {
        match self {
            Self::Request(message) => {
                hasher.update([0]);
                message.hash(hasher)
            }
            Self::Reply(message) => {
                hasher.update([1]);
                message.hash(hasher)
            }
        }
    }
}

impl DigestHash for Request {
    fn hash(&self, hasher: &mut crate::context::Hasher) {
        hasher.update(self.client_index.to_le_bytes());
        hasher.update(self.request_num.to_le_bytes());
        hasher.update(&self.op)
    }
}

impl DigestHash for Reply {
    fn hash(&self, hasher: &mut crate::context::Hasher) {
        hasher.update(self.request_num.to_le_bytes());
        hasher.update(&self.result)
    }
}
