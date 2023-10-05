use std::time::Duration;

use serde::Serialize;

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
        M: Serialize,
    {
        match self {
            Self::Tokio(context) => context.send(to, message),
            _ => unimplemented!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Signed<M> {
    Plain(M),
}

impl<M> Context<M> {
    pub fn send_signed<N, F: FnOnce(&mut N, Signed<M>)>(
        &mut self,
        to: To,
        message: M,
        loopback: impl Into<Option<F>>,
    ) {
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
