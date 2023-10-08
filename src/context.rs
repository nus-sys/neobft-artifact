use std::{collections::HashMap, net::SocketAddr, time::Duration};

use hmac::{Hmac, Mac};
use k256::{ecdsa::SigningKey, sha2::Sha256};
use serde::Serialize;

use self::{crypto::DigestHash, ordered_multicast::OrderedMulticast};

pub mod crypto;
pub mod ordered_multicast;
pub mod tokio;

pub type ReplicaIndex = u8;
pub type ClientIndex = u16;

#[derive(Debug)]
pub enum Context<M> {
    Tokio(tokio::Context),
    Phantom(std::marker::PhantomData<M>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Host {
    Client(ClientIndex),
    Replica(ReplicaIndex),
    Multicast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum To {
    Host(Host),
    AllReplica,
    Loopback,
    AllReplicaWithLoopback,
}

impl To {
    pub fn replica(index: ReplicaIndex) -> Self {
        Self::Host(Host::Replica(index))
    }

    pub fn client(index: ClientIndex) -> Self {
        Self::Host(Host::Client(index))
    }
}

impl<M> Context<M> {
    pub fn config(&self) -> &Config {
        match self {
            Self::Tokio(context) => &context.config,
            _ => unimplemented!(),
        }
    }

    pub fn send<N>(&mut self, to: To, message: N)
    where
        M: crypto::Sign<N> + Serialize,
    {
        match self {
            Self::Tokio(context) => context.send::<M, _>(to, message),
            _ => unimplemented!(),
        }
    }

    pub fn send_ordered_multicast<N>(&mut self, message: N)
    where
        N: Serialize + DigestHash,
        OrderedMulticast<N>: Into<M>,
    {
        match self {
            Self::Tokio(context) => context.send_ordered_multicast(message),
            _ => unimplemented!(),
        }
    }

    pub fn idle_hint(&self) -> bool {
        match self {
            Self::Tokio(context) => context.idle_hint(),
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

    fn handle(&mut self, receiver: Host, remote: Host, message: Self::Message);

    #[allow(unused_variables)]
    fn handle_loopback(&mut self, receiver: Host, message: Self::Message) {
        unreachable!()
    }

    fn on_timer(&mut self, receiver: Host, id: TimerId);
}

pub trait OrderedMulticastReceivers
where
    Self: Receivers,
{
    type Message;
}

#[derive(Debug, Clone)]
pub struct Config {
    pub num_faulty: usize,
    pub num_replica: usize,
    pub hosts: HashMap<Host, ConfigHost>,
    pub remotes: HashMap<SocketAddr, Host>,
    pub multicast_addr: Option<SocketAddr>,
    pub hmac: Hmac<Sha256>,
}

#[derive(Debug, Clone)]
pub struct ConfigHost {
    pub addr: SocketAddr,
    pub signing_key: Option<SigningKey>,
}

impl Config {
    pub fn new(addrs: HashMap<Host, SocketAddr>, num_faulty: usize) -> Self {
        let mut remotes = HashMap::new();
        let mut hosts = HashMap::new();
        let mut num_replica = 0;
        for (&host, &addr) in &addrs {
            remotes.insert(addr, host);
            let signing_key;
            match host {
                Host::Client(_) => signing_key = None,
                Host::Replica(index) => {
                    signing_key = Some(Self::k256(index));
                    num_replica += 1;
                }
                Host::Multicast => unimplemented!(),
            };
            hosts.insert(host, ConfigHost { addr, signing_key });
        }
        assert_eq!(remotes.len(), addrs.len());
        assert!(num_faulty * 3 < num_replica);
        Self {
            num_faulty,
            num_replica,
            hosts,
            remotes,
            multicast_addr: None,
            // simplified symmetrical keys setup
            // also reduce client-side overhead a little bit by only need to sign once for broadcast
            hmac: Hmac::new_from_slice("shared".as_bytes()).unwrap(),
        }
    }

    fn k256(index: ReplicaIndex) -> SigningKey {
        let k = format!("replica-{index}");
        let mut buf = [0; 32];
        buf[..k.as_bytes().len()].copy_from_slice(k.as_bytes());
        SigningKey::from_slice(&buf).unwrap()
    }
}
