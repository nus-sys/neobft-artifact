use std::{collections::HashMap, sync::Mutex, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    client::OnResult,
    common::{Block, BlockDigest, Chain, Request, Timer},
    context::{
        crypto::{DigestHash, Sign, Signed, Verify},
        ClientIndex, Context, Host, Receivers, To,
    },
    App,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Signed<Request>),
    Reply(Signed<Reply>),
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
        shared.context.send(To::replica(0), request)
    }

    fn handle(&self, message: Self::Message) {
        let Message::Reply(reply) = message else {
            unimplemented!()
        };
        let shared = &mut *self.shared.lock().unwrap();
        if reply.inner.request_num != shared.request_num {
            return;
        }
        shared.op.take().unwrap();
        shared.resend_timer.unset(&mut shared.context);
        shared.on_result.take().unwrap().apply(reply.inner.result);
    }
}

pub struct Replica {
    context: Context<Message>,
    blocks: HashMap<BlockDigest, Block>,
    chain: Chain,
    requests: Vec<Request>,
    replies: HashMap<ClientIndex, Reply>,
    app: App,
    pub make_blocks: bool,
}

impl Replica {
    pub fn new(context: Context<Message>, app: App) -> Self {
        Self {
            context,
            // probably need to reserve if `make_blocks` is set
            // or the rehashing will cause huge latency spikes
            blocks: HashMap::default(),
            chain: Default::default(),
            requests: Default::default(),
            replies: Default::default(),
            app,
            make_blocks: false,
        }
    }
}

impl Receivers for Replica {
    type Message = Message;

    fn handle(&mut self, receiver: Host, remote: Host, message: Self::Message) {
        assert_eq!(receiver, Host::Replica(0));
        let (Host::Client(index), Message::Request(request)) = (remote, message) else {
            unimplemented!()
        };
        match self.replies.get(&index) {
            Some(reply) if reply.request_num > request.inner.request_num => return,
            Some(reply) if reply.request_num == request.inner.request_num => {
                self.context.send(To::Host(remote), reply.clone());
                return;
            }
            _ => {}
        }

        self.requests.push(request.inner);
        if !self.make_blocks {
            let request = self.requests.last().unwrap();
            let reply = Reply {
                request_num: request.request_num,
                result: self.app.execute(&request.op),
            };
            let evicted = self.replies.insert(request.client_index, reply.clone());
            if let Some(evicted) = evicted {
                assert_eq!(evicted.request_num, request.request_num - 1)
            }
            self.context.send(To::client(request.client_index), reply)
        } else if self.context.idle_hint() {
            let block = self.chain.propose(&mut self.requests);
            assert!(block.digest() != Chain::GENESIS_DIGEST);
            let evicted = self.blocks.insert(block.digest(), block.clone());
            assert!(evicted.is_none());

            let execute = self.chain.commit(&block);
            assert!(execute);
            for request in &block.requests {
                let reply = Reply {
                    request_num: request.request_num,
                    result: self.app.execute(&request.op),
                };
                let evicted = self.replies.insert(request.client_index, reply.clone());
                if let Some(evicted) = evicted {
                    assert_eq!(evicted.request_num, request.request_num - 1)
                }
                self.context.send(To::client(request.client_index), reply)
            }
            assert!(self.chain.next_execute().is_none())
        }
    }

    fn handle_loopback(&mut self, _: Host, _: Self::Message) {
        unreachable!()
    }

    fn on_timer(&mut self, _: Host, _: crate::context::TimerId) {
        unreachable!()
    }
}

impl DigestHash for Reply {
    fn hash(&self, hasher: &mut crate::context::crypto::Hasher) {
        hasher.update(self.request_num.to_le_bytes());
        hasher.update(&self.result)
    }
}

impl Sign<Request> for Message {
    fn sign(message: Request, signer: &crate::context::crypto::Signer) -> Self {
        Self::Request(signer.sign_private(message))
    }
}

impl Sign<Reply> for Message {
    fn sign(message: Reply, signer: &crate::context::crypto::Signer) -> Self {
        Self::Reply(signer.sign_private(message))
    }
}

impl Verify for Message {
    fn verify(
        &self,
        verifier: &crate::context::crypto::Verifier,
    ) -> Result<(), crate::context::crypto::Invalid> {
        match self {
            Self::Request(message) => verifier.verify(message, None),
            Self::Reply(message) => verifier.verify(message, 0),
        }
    }
}
