use bincode::Options;
use k256::sha2::Digest;
use serde::{de::DeserializeOwned, Serialize};

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

#[derive(Debug, Clone)]
pub struct OrderedMulticast<M> {
    pub seq_num: u32,
    pub signature: OrderedMulticastSignature,
    pub linked: [u8; 32],
    pub inner: M,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderedMulticastSignature {
    HalfSipHash([[u8; 4]; 4]),
    //
}

#[derive(Debug, Clone)]
pub enum Variant {
    Unimplemented,
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
            Self::Unimplemented => unimplemented!(),
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
            inner: bincode::options().deserialize(&buf[100..]).unwrap(),
        }
    }

    pub fn verify<M>(&self, message: &OrderedMulticast<M>) -> Result<(), Invalid>
    where
        M: DigestHash,
    {
        let digest = <[_; 32]>::from(Hasher::sha256(&message.inner).finalize());
        match (self, message.signature) {
            (Self::Unimplemented, _) => unimplemented!(),
            (Self::HalfSipHash(variant), OrderedMulticastSignature::HalfSipHash(codes)) => {
                // TODO
                if digest == [0; 32] {
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
