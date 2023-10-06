use std::time::Duration;

use hmac::{Hmac, Mac};
use k256::sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

pub mod tokio;

pub type ReplicaIndex = u8;
pub type ClientIndex = u16;

pub enum Context<M> {
    Tokio(tokio::Context),
    Phantom(std::marker::PhantomData<M>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum To {
    Client(ClientIndex),
    Replica(ReplicaIndex),
    AllReplica,
}

impl<M> Context<M> {
    pub fn send(&mut self, to: To, message: M)
    where
        M: Serialize + DigestHash,
    {
        match self {
            Self::Tokio(context) => context.send(to, message),
            _ => unimplemented!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signed<M> {
    Plain(M),
    K256(M, k256::ecdsa::Signature),
    Hmac(M, [u8; 32]),
}

impl<M> Context<M> {
    pub fn send_signed<N, F: FnOnce(&mut N, Signed<M>)>(
        &mut self,
        to: To,
        message: M,
        loopback: impl Into<Option<F>>,
    ) where
        M: Serialize + DigestHash,
    {
        match self {
            Self::Tokio(context) => context.send_signed(to, message, loopback),
            _ => unimplemented!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TimerId {
    Tokio(tokio::TimerId),
}

impl<M> Context<M> {
    pub fn set(&mut self, duration: Duration) -> TimerId {
        match self {
            Self::Tokio(context) => TimerId::Tokio(context.set(duration)),
            _ => unimplemented!(),
        }
    }

    pub fn unset(&mut self, id: TimerId) {
        match (self, id) {
            (Self::Tokio(context), TimerId::Tokio(id)) => context.unset(id),
            _ => unimplemented!(),
        }
    }
}

pub trait Receivers {
    type Message;

    fn handle(&mut self, to: To, remote: To, message: Self::Message);

    fn on_timer(&mut self, to: To, id: TimerId);
}

pub enum Hasher {
    Sha256(Sha256),
    HMac(Hmac<Sha256>),
}

impl Hasher {
    pub fn update(&mut self, data: impl AsRef<[u8]>) {
        match self {
            Self::Sha256(hasher) => hasher.update(data),
            Self::HMac(hasher) => hasher.update(data.as_ref()),
        }
    }

    pub fn chain_update(self, data: impl AsRef<[u8]>) -> Self {
        match self {
            Self::Sha256(hasher) => Self::Sha256(hasher.chain_update(data)),
            Self::HMac(hasher) => Self::HMac(hasher.chain_update(data)),
        }
    }
}

pub trait DigestHash {
    fn hash(&self, hasher: &mut Hasher);
}
