use std::{collections::HashMap, sync::Arc};

use hmac::{Hmac, Mac};
use k256::{
    ecdsa::{SigningKey, VerifyingKey},
    schnorr::signature::{DigestSigner, DigestVerifier},
    sha2::{Digest, Sha256},
};
use serde::{Deserialize, Serialize};

use super::{
    ordered_multicast::{OrderedMulticast, Variant},
    Config, Host, ReplicaIndex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed<M> {
    pub inner: M,
    pub signature: Signature,
}

impl<M> std::ops::Deref for Signed<M> {
    type Target = M;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
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

impl std::hash::Hasher for Hasher {
    fn write(&mut self, buf: &[u8]) {
        self.update(buf)
    }

    fn finish(&self) -> u64 {
        unimplemented!()
    }
}

pub trait DigestHash {
    fn hash(&self, hasher: &mut impl std::hash::Hasher);
}

impl<T: DigestHash> DigestHash for [T] {
    fn hash(&self, hasher: &mut impl std::hash::Hasher) {
        for item in self {
            item.hash(hasher)
        }
    }
}

impl Hasher {
    pub fn sha256(message: &impl DigestHash) -> Sha256 {
        let mut hasher = Self::Sha256(Sha256::new());
        message.hash(&mut hasher);
        let Self::Sha256(digest) = hasher else {
            unreachable!()
        };
        digest
    }

    pub fn sha256_update(message: &impl DigestHash, digest: &mut Sha256) {
        let mut hasher = Self::Sha256(digest.clone());
        message.hash(&mut hasher);
        if let Self::Sha256(new_digest) = hasher {
            *digest = new_digest
        } else {
            unreachable!()
        };
    }

    pub fn hmac(message: &impl DigestHash, hmac: Hmac<Sha256>) -> [u8; 32] {
        let mut hasher = Self::Hmac(hmac);
        message.hash(&mut hasher);
        let Self::Hmac(hmac) = hasher else {
            unreachable!()
        };
        hmac.finalize().into_bytes().into()
    }
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
        let digest = Hasher::sha256(&message);
        Signed {
            inner: message,
            signature: Signature::K256(self.signing_key.as_ref().unwrap().sign_digest(digest)),
        }
    }

    pub fn sign_private<M>(&self, message: M) -> Signed<M>
    where
        M: DigestHash,
    {
        Signed {
            signature: Signature::Hmac(Hasher::hmac(&message, self.hmac.clone())),
            inner: message,
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
    variant: Arc<Variant>,
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
    pub fn new_standard(config: &Config, variant: Arc<Variant>) -> Self {
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
            variant,
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
            (Self::Standard(verifier), Signature::K256(signature)) => verifier.verifying_keys
                [&index.into().unwrap()]
                .verify_digest(Hasher::sha256(&**message), signature)
                .map_err(|_| Invalid::Public),
            (Self::Standard(verifier), Signature::Hmac(code)) => {
                // well...
                let mut hasher = Hasher::Hmac(verifier.hmac.clone());
                message.hash(&mut hasher);
                let Hasher::Hmac(hmac) = hasher else {
                    unreachable!()
                };
                hmac.verify(code.into()).map_err(|_| Invalid::Private)
            }
        }
    }

    pub fn verify_ordered_multicast<M>(&self, message: &OrderedMulticast<M>) -> Result<(), Invalid>
    where
        M: DigestHash,
    {
        match self {
            Self::Nop => Ok(()),
            Self::Standard(verifier) => verifier.variant.verify(message),
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
