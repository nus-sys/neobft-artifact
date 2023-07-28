use std::{
    hash::{Hash, Hasher},
    net::SocketAddr,
};

use dsys::{
    protocol::ReactiveGenerate,
    udp::{RxEvent, TxEvent},
    Protocol,
};
use secp256k1::{Secp256k1, SecretKey, SignOnly};
use sha2::Digest;
use siphasher::sip::SipHasher;

pub struct Sequencer {
    seq: u32,
    link_hash: [u8; 32],
    pub should_link: Box<dyn FnMut() -> bool + Send>,
}

impl Default for Sequencer {
    fn default() -> Self {
        Self {
            seq: 0,
            link_hash: Default::default(),
            should_link: Box::new(|| false),
        }
    }
}

impl Protocol<RxEvent<'_>> for Sequencer {
    type Effect = (Box<[u8]>, bool);

    fn update(&mut self, event: RxEvent<'_>) -> Self::Effect {
        let RxEvent::Receive(buf) = event;
        let mut buf = <Box<[_]>>::from(buf);
        self.seq += 1;
        buf[..4].copy_from_slice(&self.seq.to_be_bytes());
        buf[32..64].copy_from_slice(&self.link_hash);
        self.link_hash = sha2::Sha256::digest(&buf[..64]).into();
        let should_link = (self.should_link)();
        if should_link {
            buf.copy_within(32..64, 4);
            buf[36..68].copy_from_slice(&[0; 32]);
        }
        (buf, should_link)
    }
}

pub struct SipHash {
    pub multicast_addr: SocketAddr,
    pub replica_count: u8,
}

impl ReactiveGenerate<Box<[u8]>> for SipHash {
    type Event = TxEvent;

    fn update<P>(&mut self, buf: Box<[u8]>, protocol: &mut P)
    where
        P: for<'a> Protocol<Self::Event, Effect = ()>,
    {
        let mut index = 0;
        while index < self.replica_count {
            let mut buf = buf.clone();
            let mut signatures = [0; 16];
            for j in index..u8::min(index + 4, self.replica_count) {
                let mut hasher = SipHasher::new_with_keys(u64::MAX, j as _);
                buf[..32].hash(&mut hasher);

                let offset = (j - index) as usize * 4;
                signatures[offset..offset + 4].copy_from_slice(&hasher.finish().to_le_bytes()[..4]);
                // println!("signature[{j}] {:02x?}", &signatures[offset..offset + 4]);
            }
            buf[4] = index;
            buf[5..21].copy_from_slice(&signatures);

            protocol.update(TxEvent::Send(self.multicast_addr, buf));
            index += 4;
        }
    }
}

pub struct P256 {
    multicast_addr: SocketAddr,
    secret_key: SecretKey,
    secp: Secp256k1<SignOnly>,
}

impl P256 {
    pub fn new(multicast_addr: SocketAddr, secret_key: SecretKey) -> Self {
        Self {
            multicast_addr,
            secret_key,
            secp: Secp256k1::signing_only(),
        }
    }
}

impl Protocol<(Box<[u8]>, bool)> for P256 {
    type Effect = TxEvent;

    fn update(&mut self, (mut buf, should_link): (Box<[u8]>, bool)) -> Self::Effect {
        if !should_link {
            let message = secp256k1::Message::from_slice(&buf[..32]).unwrap();
            let signature = self
                .secp
                .sign_ecdsa(&message, &self.secret_key)
                .serialize_compact();

            buf[4..68].copy_from_slice(&signature);
        }
        TxEvent::Send(self.multicast_addr, buf)
    }
}
