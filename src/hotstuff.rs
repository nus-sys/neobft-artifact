use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{
    client::BoxedConsume,
    common::{Block, BlockDigest, Chain, Request, Timer},
    context::{
        crypto::{DigestHash, Sign, Signed, Verify},
        ClientIndex, Host, Receivers, ReplicaIndex, To,
    },
    App, Context,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Signed<Request>),
    Reply(Signed<Reply>),
    Generic(Signed<Generic>),
    Vote(Signed<Vote>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    request_num: u32,
    result: Vec<u8>,
    replica_index: ReplicaIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generic {
    block: Block,
    certified_digest: BlockDigest,
    certificate: Vec<Signed<Vote>>,
    replica_index: ReplicaIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    block_digest: BlockDigest,
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
        shared.context.send(To::AllReplica, request);
        shared.resend_timer.set(&mut shared.context)
    }

    fn handle(&self, message: Self::Message) {
        let Message::Reply(message) = message else {
            unimplemented!()
        };
        let shared = &mut *self.shared.lock().unwrap();
        if message.request_num != shared.request_num {
            return;
        }
        let Some(invoke) = &mut shared.invoke else {
            return;
        };
        invoke
            .replies
            .insert(message.replica_index, Reply::clone(&message));
        let num_match = invoke
            .replies
            .values()
            .filter(|reply| reply.result == message.result)
            .count();
        assert!(num_match <= shared.context.config().num_faulty + 1);
        if num_match == shared.context.config().num_faulty + 1 {
            shared.resend_timer.unset(&mut shared.context);
            let invoke = shared.invoke.take().unwrap();
            let _op = invoke.op;
            invoke.consume.apply(message.inner.result)
        }
    }
}

#[allow(unused)]
pub struct Replica {
    context: Context<Message>,
    index: ReplicaIndex,
    requests: Vec<Request>,
    generics: HashMap<BlockDigest, Signed<Generic>>,
    votes: HashMap<BlockDigest, HashMap<ReplicaIndex, Signed<Vote>>>,
    certified_digest: BlockDigest,
    reordering_generics: HashMap<BlockDigest, Vec<Signed<Generic>>>,
    chain: Chain,
    app: App,
}

impl Replica {
    pub fn new(context: Context<Message>, index: ReplicaIndex, app: App) -> Self {
        let mut votes = HashMap::new();
        votes.insert(Chain::GENESIS_DIGEST, Default::default());
        Self {
            context,
            index,
            requests: Default::default(),
            generics: Default::default(),
            votes,
            certified_digest: Chain::GENESIS_DIGEST,
            reordering_generics: Default::default(),
            chain: Default::default(),
            app,
        }
    }
}

impl Receivers for Replica {
    type Message = Message;

    fn handle(&mut self, receiver: Host, remote: Host, message: Self::Message) {
        assert_eq!(receiver, Host::Replica(self.index));
        match message {
            Message::Request(message) => self.handle_request(remote, message),
            Message::Generic(message) => self.handle_generic(remote, message),
            Message::Vote(message) => self.handle_vote(remote, message),
            _ => unimplemented!(),
        }
    }

    fn on_timer(&mut self, receiver: Host, _: crate::context::TimerId) {
        assert_eq!(receiver, Host::Replica(self.index));
        todo!()
    }

    fn on_pace(&mut self) {
        if self.index == self.primary_index() && !self.requests.is_empty() {
            self.do_propose()
        }
    }
}

impl Replica {
    fn primary_index(&self) -> ReplicaIndex {
        0 // TODO rotate
    }

    fn handle_request(&mut self, _remote: Host, message: Signed<Request>) {
        // TODO check resend

        if self.index != self.primary_index() {
            return;
        }

        self.requests.push(message.inner)
    }

    fn handle_generic(&mut self, _remote: Host, _message: Signed<Generic>) {
        //
    }

    fn handle_vote(&mut self, _remote: Host, message: Signed<Vote>) {
        let block_digest = message.block_digest;
        let votes = self.votes.entry(block_digest).or_default();
        if votes.len() == self.context.config().num_replica - self.context.config().num_faulty {
            return;
        }
        votes.insert(message.replica_index, message);
        if votes.len() == self.context.config().num_replica - self.context.config().num_faulty {
            self.certified_digest = block_digest;
            if self.index == self.primary_index() && !self.requests.is_empty() {
                self.do_propose()
            }
        }
    }

    fn do_propose(&mut self) {
        let generic = Generic {
            replica_index: self.index,
            block: self.chain.propose(&mut self.requests),
            certified_digest: self.certified_digest,
            certificate: self.votes[&self.certified_digest]
                .values()
                .cloned()
                .collect(),
        };
        self.context.send(To::AllReplicaWithLoopback, generic)
    }
}

impl DigestHash for Reply {
    fn hash(&self, hasher: &mut impl std::hash::Hasher) {
        hasher.write_u32(self.request_num);
        hasher.write(&self.result);
        hasher.write_u8(self.replica_index)
    }
}

impl DigestHash for Generic {
    fn hash(&self, hasher: &mut impl std::hash::Hasher) {
        self.block.hash(hasher);
        hasher.write(&self.certified_digest);
        self.certificate.hash(hasher);
        hasher.write_u8(self.replica_index)
    }
}

impl DigestHash for Vote {
    fn hash(&self, hasher: &mut impl std::hash::Hasher) {
        hasher.write(&self.block_digest);
        hasher.write_u8(self.replica_index)
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

impl Sign<Generic> for Message {
    fn sign(message: Generic, signer: &crate::context::crypto::Signer) -> Self {
        Self::Generic(signer.sign_public(message))
    }
}

impl Sign<Vote> for Message {
    fn sign(message: Vote, signer: &crate::context::crypto::Signer) -> Self {
        Self::Vote(signer.sign_public(message))
    }
}

impl Verify for Message {
    fn verify(
        &self,
        verifier: &crate::context::crypto::Verifier,
    ) -> Result<(), crate::context::crypto::Invalid> {
        match self {
            Self::Request(message) => verifier.verify(message, None),
            Self::Reply(message) => verifier.verify(message, message.replica_index),
            Self::Generic(message) => {
                verifier.verify(message, message.replica_index)?;
                if message.certified_digest == Chain::GENESIS_DIGEST {
                    return Ok(());
                }
                // TODO check certification size
                for vote in &message.certificate {
                    verifier.verify(vote, vote.replica_index)?
                }
                Ok(())
            }
            Self::Vote(message) => verifier.verify(message, message.replica_index),
        }
    }
}
