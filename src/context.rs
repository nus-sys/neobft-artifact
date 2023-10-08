use std::{collections::HashMap, net::SocketAddr, time::Duration};

use hmac::{Hmac, Mac};
use k256::{ecdsa::SigningKey, sha2::Sha256};
use serde::Serialize;

use self::ordered_multicast::OrderedMulticast;

pub mod crypto;
pub mod ordered_multicast;
pub mod tokio;

pub type ReplicaIndex = u8;
pub type ClientIndex = u16;

pub enum Context<M> {
    Tokio(tokio::Context),
    Phantom(std::marker::PhantomData<M>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Host {
    Client(ClientIndex),
    Replica(ReplicaIndex),
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
    pub fn send<N>(&mut self, to: To, message: N)
    where
        M: crypto::Sign<N> + Serialize,
    {
        match self {
            Self::Tokio(context) => context.send::<M, _>(to, message),
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

    fn handle_loopback(&mut self, receiver: Host, message: Self::Message);

    fn on_timer(&mut self, receiver: Host, id: TimerId);
}

pub trait OrderedMulticastReceivers {
    type Message;

    fn handle(&mut self, remote: Host, message: OrderedMulticast<Self::Message>);
}

#[derive(Debug, Clone)]
pub struct Config {
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
    pub fn new(addrs: HashMap<Host, SocketAddr>) -> Self {
        let remotes = HashMap::from_iter(addrs.iter().map(|(&host, &addr)| (addr, host)));
        assert_eq!(remotes.len(), addrs.len());
        let hosts = HashMap::from_iter(addrs.into_iter().map(|(host, addr)| {
            (
                host,
                ConfigHost {
                    addr,
                    signing_key: match host {
                        Host::Client(_) => None,
                        Host::Replica(index) => Some(Self::k256(index)),
                    },
                },
            )
        }));
        Self {
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
