//! A context based on tokio and asynchronous IO.
//!
//! Although the event management is asynchronous, protocol code, i.e.,
//! `impl Receivers` is still synchronous and running in a separated thread.

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use bincode::Options;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{net::UdpSocket, runtime::Handle, task::JoinHandle};

use super::{Signed, To};

#[derive(Debug, Clone)]
pub struct Config {
    pub addrs: HashMap<To, SocketAddr>,
    pub remotes: HashMap<SocketAddr, To>,
    // keys
}

#[derive(Debug)]
pub struct Context {
    config: Arc<Config>,
    socket: Arc<UdpSocket>,
    runtime: Handle,
    source: To,
    timer_id: TimerId,
    timer_sender: flume::Sender<(To, TimerId)>,
    timer_tasks: HashMap<TimerId, JoinHandle<()>>,
}

impl Context {
    pub fn send(&self, to: To, message: impl Serialize) {
        let buf = bincode::options().serialize(&message).unwrap();
        let config = self.config.clone();
        let socket = self.socket.clone();
        let source = self.source;
        self.runtime.spawn(async move {
            match to {
                To::Client(_) | To::Replica(_) => {
                    assert_ne!(to, source);
                    socket.send_to(&buf, config.addrs[&to]).await.unwrap();
                }
                To::AllReplica => {
                    for (&to, addr) in &config.addrs {
                        if matches!(to, To::Replica(_)) && to != source {
                            socket.send_to(&buf, addr).await.unwrap();
                        }
                    }
                }
            }
        });
    }

    pub fn send_signed<M, N, F: FnOnce(&mut N, Signed<M>)>(
        &self,
        to: To,
        message: M,
        loopback: impl Into<Option<F>>,
    ) {
        todo!()
    }
}

pub type TimerId = u32;

impl Context {
    pub fn set(&mut self, duration: Duration) -> TimerId {
        self.timer_id += 1;
        let id = self.timer_id;
        let sender = self.timer_sender.clone();
        let source = self.source;
        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(duration).await;
                sender.send_async((source, id)).await.unwrap()
            }
        });
        self.timer_tasks.insert(id, task);
        id
    }

    pub fn unset(&mut self, id: TimerId) {
        self.timer_tasks.remove(&id).unwrap().abort()
    }
}

pub struct Runtime {
    config: Arc<Config>,
    runtime: Handle,
    message_sender: flume::Sender<(To, To, Vec<u8>)>,
    message_receiver: flume::Receiver<(To, To, Vec<u8>)>,
    timer_sender: flume::Sender<(To, TimerId)>,
    timer_receiver: flume::Receiver<(To, TimerId)>,
}

impl Runtime {
    pub fn new(config: Config, runtime: Handle) -> Self {
        let (message_sender, message_receiver) = flume::unbounded();
        let (timer_sender, timer_receiver) = flume::bounded(0);
        Self {
            config: Arc::new(config),
            runtime,
            message_sender,
            message_receiver,
            timer_sender,
            timer_receiver,
        }
    }

    pub fn register<M>(&self, receiver: To) -> super::Context<M> {
        let socket = Arc::new(
            self.runtime
                .block_on(UdpSocket::bind(self.config.addrs[&receiver]))
                .unwrap(),
        );
        let context = Context {
            config: self.config.clone(),
            socket: socket.clone(),
            runtime: self.runtime.clone(),
            source: receiver,
            timer_id: Default::default(),
            timer_sender: self.timer_sender.clone(),
            timer_tasks: Default::default(),
        };
        let message_sender = self.message_sender.clone();
        let config = self.config.clone();
        self.runtime.spawn(async move {
            let mut buf = vec![0; 65536];
            loop {
                let (len, remote) = socket.recv_from(&mut buf).await.unwrap();
                message_sender
                    .try_send((receiver, config.remotes[&remote], buf[..len].to_vec()))
                    .unwrap()
            }
        });
        super::Context::Tokio(context)
    }
}

pub trait Receivers {
    type Message;

    fn handle(&mut self, to: To, remote: To, message: Self::Message);

    fn on_timer(&mut self, to: To, id: TimerId);
}

impl Runtime {
    pub fn run<M>(&self, receivers: &mut impl Receivers<Message = M>)
    where
        M: DeserializeOwned,
    {
        enum Event {
            Message(To, To, Vec<u8>),
            Timer(To, TimerId),
        }

        loop {
            let event = flume::Selector::new()
                .recv(&self.message_receiver, |event| {
                    let (to, remote, message) = event.unwrap();
                    Event::Message(to, remote, message)
                })
                .recv(&self.timer_receiver, |event| {
                    let (to, id) = event.unwrap();
                    Event::Timer(to, id)
                })
                .wait();
            match event {
                Event::Message(to, remote, message) => {
                    let message = bincode::options()
                        .allow_trailing_bytes()
                        .deserialize(&message)
                        .unwrap();
                    receivers.handle(to, remote, message)
                }
                Event::Timer(to, id) => receivers.on_timer(to, id),
            }
        }
    }
}
