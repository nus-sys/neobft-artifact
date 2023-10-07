use std::collections::HashMap;

use hmac::{Hmac, Mac};
use k256::{
    ecdsa::{SigningKey, VerifyingKey},
    schnorr::signature::{DigestSigner, DigestVerifier},
    sha2::{Digest, Sha256},
};
use serde::{Deserialize, Serialize};

use super::{tokio::Config, Host, ReplicaIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed<M> {
    pub inner: M,
    pub signature: Signature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signature {
    Plain,
    K256(k256::ecdsa::Signature),
    Hmac([u8; 32]),
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
    pub signing_key: Option<SigningKey>,
    pub hmac: Hmac<Sha256>,
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
        Signed {
            inner: message,
            signature: Signature::K256(self.signing_key.as_ref().unwrap().sign_digest(digest)),
        }
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
        Signed {
            inner: message,
            signature: Signature::Hmac(hmac.finalize().into_bytes().into()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Verifier {
    Nop,
    Standard(Box<VerifierStandard>),
}

#[derive(Debug, Clone)]
pub struct VerifierStandard {
    verifying_keys: HashMap<ReplicaIndex, VerifyingKey>,
    hmac: Hmac<Sha256>,
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
    pub fn new_standard(config: &Config) -> Self {
        let verifying_keys = config
            .hosts
            .iter()
            .filter_map(|(&host, host_config)| {
                if let Host::Replica(index) = host {
                    Some((
                        index,
                        *host_config.signing_key.as_ref().unwrap().verifying_key(),
                    ))
                } else {
                    None
                }
            })
            .collect();
        Self::Standard(Box::new(VerifierStandard {
            verifying_keys,
            hmac: config.hmac.clone(),
        }))
    }

    pub fn verify<M>(
        &self,
        message: &Signed<M>,
        index: impl Into<Option<ReplicaIndex>>,
    ) -> Result<(), Invalid>
    where
        M: DigestHash,
    {
        match (self, &message.signature) {
            (Self::Nop, _) => Ok(()),
            (Self::Standard(_), Signature::Plain) => unimplemented!(),
            (Self::Standard(verifier), Signature::K256(signature)) => {
                let mut hasher = Hasher::Sha256(Sha256::new());
                message.inner.hash(&mut hasher);
                let Hasher::Sha256(digest) = hasher else {
                    unreachable!()
                };
                verifier.verifying_keys[&index.into().unwrap()]
                    .verify_digest(digest, signature)
                    .map_err(|_| Invalid::Public)
            }
            (Self::Standard(verifier), Signature::Hmac(code)) => {
                let mut hasher = Hasher::Hmac(verifier.hmac.clone());
                message.inner.hash(&mut hasher);
                let Hasher::Hmac(hmac) = hasher else {
                    unreachable!()
                };
                hmac.verify(code.into()).map_err(|_| Invalid::Private)
            }
        }
    }
}

pub trait Sign<M> {
    fn sign(message: M, signer: &Signer) -> Self;
}

impl<M, N: Into<M>> Sign<N> for M {
    fn sign(message: N, _: &Signer) -> Self {
        message.into()
    }
}

pub trait Verify {
    fn verify(&self, verifier: &Verifier) -> Result<(), Invalid>;
}
