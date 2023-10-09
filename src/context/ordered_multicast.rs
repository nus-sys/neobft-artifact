use bincode::Options;
use k256::sha2::Digest;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{
    crypto::{DigestHash, Hasher, Invalid},
    ReplicaIndex,
};

pub fn serialize(message: &(impl Serialize + DigestHash)) -> Vec<u8> {
    let digest = Hasher::sha256(message).finalize();
    [
        &[0; 20],
        &digest[..8], // read by HalfSipHash
        &[0; 40],
        &*digest, // read by K256
        &bincode::options().serialize(message).unwrap(),
    ]
    .concat()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderedMulticast<M> {
    pub seq_num: u32,
    pub signature: OrderedMulticastSignature,
    pub linked: [u8; 32],
    pub inner: M,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderedMulticastSignature {
    HalfSipHash([[u8; 4]; 4]),
    //
}

impl<M> std::ops::Deref for OrderedMulticast<M> {
    type Target = M;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DigestHash for OrderedMulticastSignature {
    fn hash(&self, hasher: &mut impl std::hash::Hasher) {
        match self {
            Self::HalfSipHash([code0, code1, code2, code3]) => {
                hasher.write(&code0[..]);
                hasher.write(&code1[..]);
                hasher.write(&code2[..]);
                hasher.write(&code3[..])
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Variant {
    Unreachable,
    HalfSipHash(HalfSipHash),
    K256,
}

#[derive(Debug, Clone)]
pub struct HalfSipHash {
    index: ReplicaIndex,
    //
}

impl Variant {
    pub fn new_half_sip_hash(index: ReplicaIndex) -> Self {
        Self::HalfSipHash(HalfSipHash { index })
    }

    pub fn deserialize<M>(&self, buf: impl AsRef<[u8]>) -> OrderedMulticast<M>
    where
        M: DeserializeOwned,
    {
        let buf = buf.as_ref();
        let mut seq_num = [0; 4];
        seq_num.copy_from_slice(&buf[0..4]);
        let signature = match self {
            Self::Unreachable => unreachable!(),
            Self::HalfSipHash(_) => {
                let mut codes = [[0; 4]; 4];
                codes[0].copy_from_slice(&buf[4..8]);
                codes[1].copy_from_slice(&buf[8..12]);
                codes[2].copy_from_slice(&buf[12..16]);
                codes[3].copy_from_slice(&buf[16..20]);
                OrderedMulticastSignature::HalfSipHash(codes)
            }
            Self::K256 => todo!(),
        };
        let mut linked = [0; 32];
        if matches!(self, Self::K256) {
            linked.copy_from_slice(&buf[68..100]);
        }
        OrderedMulticast {
            seq_num: u32::from_be_bytes(seq_num),
            signature,
            linked,
            inner: bincode::options()
                .allow_trailing_bytes()
                .deserialize(&buf[100..])
                .unwrap(),
        }
    }

    pub fn verify<M>(&self, message: &OrderedMulticast<M>) -> Result<(), Invalid>
    where
        M: DigestHash,
    {
        let digest = <[_; 32]>::from(Hasher::sha256(&**message).finalize());
        match (self, message.signature) {
            (Self::Unreachable, _) => unreachable!(),
            (Self::HalfSipHash(variant), OrderedMulticastSignature::HalfSipHash(codes)) => {
                // TODO tentatively mock the HalfSipHash for SipHash
                use std::hash::BuildHasher;
                if std::collections::hash_map::RandomState::new().hash_one(digest) == 0 {
                    return Err(Invalid::Private);
                }
                if codes[variant.index as usize] == [0; 4] {
                    return Err(Invalid::Private);
                }
                Ok(())
            }
            (Self::K256, _) => todo!(),
            // _ => unimplemented!(),
        }
    }
}
