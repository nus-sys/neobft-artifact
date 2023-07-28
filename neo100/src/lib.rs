pub mod client;
pub mod replica;
pub mod seq;

use std::{
    hash::{Hash, Hasher},
    net::{Ipv4Addr, SocketAddr, UdpSocket},
};

pub use crate::client::Client;
pub use crate::replica::Replica;
pub use crate::seq::Sequencer;

use bincode::Options;
use dsys::{
    protocol::Multiplex,
    udp::{RxEvent, TxEvent},
    NodeAddr, NodeEffect, Protocol,
};
use secp256k1::{ecdsa::Signature, PublicKey, Secp256k1, VerifyOnly};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use siphasher::sip::SipHasher;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    client_id: u32,
    client_addr: NodeAddr,
    request_num: u32,
    op: Box<[u8]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Multicast {
    seq: u32,
    crypto: MulticastCrypto,
    digest: [u8; 32], // precomputed for replica
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MulticastCrypto {
    SipHash {
        index: u8,
        signatures: [[u8; 4]; 4],
    },
    P256 {
        link_hash: Option<[u8; 32]>,
        // do this split so `MulticastCrypto` can be `Serialize`
        // so `Multicast`, `Message::OrderedRequest` can be `Serialize`
        // so `OrderedRequest` can be kept as a variant of `Message`
        // so replica can have a simple `NodeEvent<Message>` as event
        // thus, it can only be `[u8; 64]` if we don't need the last one
        signature: Option<([u8; 32], [u8; 32])>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    request_num: u32,
    result: Box<[u8]>,
    replica_id: u8,
    seq: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    OrderedRequest(Multicast, Request),
    Reply(Reply),
}

impl Multicast {
    fn digest(buf: &[u8]) -> [u8; 32] {
        let mut digest = <[u8; 32]>::from(sha2::Sha256::digest(&buf[68..]));
        digest[..4].copy_from_slice(&buf[..4]);
        digest
    }

    fn deserialize_sip_hash(buf: &[u8]) -> Multicast {
        let seq = u32::from_be_bytes(buf[..4].try_into().unwrap());
        let index = buf[4];
        let mut signatures = [[0; 4]; 4];
        for (i, signature) in signatures.iter_mut().enumerate() {
            let offset = 5 + i * 4;
            signature.copy_from_slice(&buf[offset..offset + 4]);
        }
        Multicast {
            seq,
            crypto: MulticastCrypto::SipHash { index, signatures },
            digest: Self::digest(buf),
        }
    }

    fn deserialize_p256(buf: &[u8]) -> Multicast {
        let seq = u32::from_be_bytes(buf[..4].try_into().unwrap());
        let part_a = <[u8; 32]>::try_from(&buf[4..36]).unwrap();
        let part_b = <[u8; 32]>::try_from(&buf[36..68]).unwrap();
        let crypto = if part_b == [0; 32] {
            MulticastCrypto::P256 {
                link_hash: Some(part_a),
                signature: None,
            }
        } else {
            MulticastCrypto::P256 {
                link_hash: None,
                signature: Some((part_a, part_b)),
            }
        };
        Multicast {
            seq,
            crypto,
            digest: Self::digest(buf),
        }
    }
}

impl Message {
    pub fn is_unicast(buf: &[u8]) -> bool {
        buf[..4] == [0; 4]
    }

    fn ordered_request(multicast: Multicast, buf: &[u8]) -> Option<Self> {
        if let Ok(Message::Request(request)) =
            bincode::options().allow_trailing_bytes().deserialize(buf)
        {
            Some(Message::OrderedRequest(multicast, request))
        } else {
            eprintln!("malformed (deserialize)");
            None
        }
    }
}

pub fn init_socket(socket: &UdpSocket, multicast_ip: Option<Ipv4Addr>) {
    dsys::udp::init_socket(socket);
    if let Some(multicast_ip) = multicast_ip {
        assert!(multicast_ip.is_multicast());
        socket
            .join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)
            .unwrap();
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MulticastCryptoMethod {
    SipHash { id: u8 },
    P256,
}

pub enum Rx {
    UnicastOnly,
    Multicast(MulticastCryptoMethod),
}

pub enum RxP256Event {
    Unicast(Message),
    Multicast(Multicast, Box<[u8]>), // 68..
}

impl Protocol<RxEvent<'_>> for Rx {
    // A: verified/dropped, B: need RxP256 to help verify
    type Effect = Multiplex<Option<Message>, RxP256Event>;

    fn update(&mut self, event: RxEvent<'_>) -> Self::Effect {
        let RxEvent::Receive(buf) = event;
        // println!("receive {buf:02x?}");
        if Message::is_unicast(&buf) {
            return Multiplex::B(RxP256Event::Unicast(
                // not feel good to repeat this chain
                // maybe include a `NodeRx<Message>` here?
                bincode::options()
                    .allow_trailing_bytes()
                    .deserialize(&buf[4..])
                    .unwrap(),
            ));
        }
        let multicast;
        match self {
            Rx::UnicastOnly => return Multiplex::A(None),
            Rx::Multicast(MulticastCryptoMethod::SipHash { id }) => {
                multicast = Multicast::deserialize_sip_hash(&buf);
                let MulticastCrypto::SipHash { index, signatures } = &multicast.crypto else {
                    unreachable!()
                };
                if (*index..*index + 4).contains(id) {
                    let mut hasher = SipHasher::new_with_keys(u64::MAX, *id as _);
                    multicast.digest.hash(&mut hasher);
                    let expect = &hasher.finish().to_le_bytes()[..4];
                    if signatures[(*id - *index) as usize] != expect {
                        eprintln!(
                            "malformed (siphash) expect {expect:02x?} actual {:02x?}",
                            signatures[(*id - *index) as usize],
                        );
                        return Multiplex::A(None);
                    }
                }
                // otherwise, there is no MAC for this receiver in the packet
            }
            Rx::Multicast(MulticastCryptoMethod::P256) => {
                multicast = Multicast::deserialize_p256(&buf);
                let MulticastCrypto::P256 { signature, .. } = &multicast.crypto else {
                    unreachable!()
                };
                if signature.is_some() {
                    return Multiplex::B(RxP256Event::Multicast(multicast, buf[68..].into()));
                }
            }
        }
        Multiplex::A(Message::ordered_request(multicast, &buf[68..]))
    }
}

pub struct RxP256 {
    multicast: Option<PublicKey>,
    // replica keys
    secp: Secp256k1<VerifyOnly>,
}

impl RxP256 {
    pub fn new(multicast: Option<PublicKey>) -> Self {
        Self {
            multicast,
            secp: Secp256k1::verification_only(),
        }
    }
}

impl Protocol<RxP256Event> for RxP256 {
    type Effect = Option<Message>;

    fn update(&mut self, event: RxP256Event) -> Self::Effect {
        match event {
            RxP256Event::Unicast(message) => {
                // TODO verify cross replica messages
                Some(message)
            }
            RxP256Event::Multicast(multicast, buf) => {
                let Some(public_key) = &self.multicast else {
                    panic!()
                };
                let MulticastCrypto::P256 { link_hash: None, signature: Some(signature) } = &multicast.crypto else {
                    unreachable!()
                };

                let message = secp256k1::Message::from_slice(&multicast.digest).unwrap();
                let mut bytes = [0; 64];
                bytes[..32].copy_from_slice(&signature.0);
                bytes[32..].copy_from_slice(&signature.1);
                if Signature::from_compact(&bytes)
                    .map(|signature| self.secp.verify_ecdsa(&message, &signature, public_key))
                    .is_err()
                {
                    eprintln!("malformed (p256)");
                    None
                } else {
                    Message::ordered_request(multicast, &buf)
                }
            }
        }
    }
}

pub struct Tx {
    // if send to this address, include full 96 bytes multicast header in payload
    pub multicast: Option<SocketAddr>,
}

impl Protocol<NodeEffect<Message>> for Tx {
    type Effect = TxEvent;

    fn update(&mut self, event: NodeEffect<Message>) -> Self::Effect {
        // TODO extract common serialization as `Message` method
        match event {
            NodeEffect::Broadcast(message) => {
                let buf = bincode::options().serialize(&message).unwrap();
                TxEvent::Broadcast([&[0; 4][..], &buf].concat().into())
            }
            NodeEffect::Send(NodeAddr::Socket(addr), message) if Some(addr) != self.multicast => {
                let buf = bincode::options().serialize(&message).unwrap();
                // println!("unicast {addr} {buf:02x?}");
                TxEvent::Send(addr, [&[0; 4][..], &buf].concat().into())
            }
            NodeEffect::Send(NodeAddr::Socket(_), message) => {
                let buf = bincode::options().serialize(&message).unwrap();
                let digest = <[u8; 32]>::from(sha2::Sha256::digest(&buf));
                TxEvent::Send(
                    self.multicast.unwrap(),
                    [&digest[..], &[0; 36][..], &buf].concat().into(),
                )
            }
            NodeEffect::Send(..) => panic!(),
        }
    }
}
