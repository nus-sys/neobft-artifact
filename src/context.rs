use std::time::Duration;

use hmac::{Hmac, Mac};
use k256::{
    ecdsa::SigningKey,
    schnorr::signature::DigestSigner,
    sha2::{Digest, Sha256},
};
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
    Hmac(Hmac<Sha256>),
}

impl Hasher {
    pub fn update(&mut self, data: impl AsRef<[u8]>) {
        match self {
            Self::Sha256(hasher) => hasher.update(data),
            Self::Hmac(hasher) => hasher.update(data.as_ref()),
        }
    }

    pub fn chain_update(self, data: impl AsRef<[u8]>) -> Self {
        match self {
            Self::Sha256(hasher) => Self::Sha256(hasher.chain_update(data)),
            Self::Hmac(hasher) => Self::Hmac(hasher.chain_update(data)),
        }
    }
}

pub trait DigestHash {
    fn hash(&self, hasher: &mut Hasher);
}

#[derive(Debug, Clone)]
pub struct Signer {
    signing_key: SigningKey,
    hmac: Hmac<Sha256>,
}

impl Signer {
    pub fn sign_public<M>(&self, message: M) -> Signed<M>
    where
        M: DigestHash,
    {
        let mut hasher = Hasher::Sha256(Sha256::new());
        message.hash(&mut hasher);
        let Hasher::Sha256(digest) = hasher else {
            unreachable!()
        };
        Signed::K256(message, self.signing_key.sign_digest(digest))
    }

    pub fn sign_private<M>(&self, message: M) -> Signed<M>
    where
        M: DigestHash,
    {
        let mut hasher = Hasher::Hmac(self.hmac.clone());
        message.hash(&mut hasher);
        let Hasher::Hmac(hmac) = hasher else {
            unreachable!()
        };
        Signed::Hmac(message, hmac.finalize().into_bytes().into())
    }
}

#[derive(Debug, Clone)]
pub enum Verifier {
    //
}

#[derive(Debug, Clone, Copy)]
pub enum Invalid {
    Public,
    Private,
}

impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Invalid {}

impl Verifier {
    pub fn verify<M>(&mut self, message: &Signed<M>) -> Result<(), Invalid>
    where
        M: DigestHash,
    {
        Ok(())
    }
}

pub trait Sign<M> {
    fn sign(message: M, signer: &Signer) -> Self;
}

pub trait Verify {
    fn verify(&self, verifier: &Verifier) -> Result<(), Invalid>;
}
