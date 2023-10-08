use std::time::Duration;

use serde::Serialize;

pub mod crypto;
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
