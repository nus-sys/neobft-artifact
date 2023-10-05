//! A context based on tokio and asynchronous IO.
//!
//! Although the event management is asynchronous, protocol code, i.e.,
//! `impl Receivers` is still synchronous and running in a separated thread.

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use bincode::Options;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{net::UdpSocket, runtime::Handle, task::JoinHandle};

use super::{Receivers, Signed, To};

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub addrs: HashMap<To, SocketAddr>,
    pub remotes: HashMap<SocketAddr, To>,
    // keys
}

impl Config {
    pub fn new(addrs: HashMap<To, SocketAddr>) -> Self {
        let remotes = HashMap::from_iter(addrs.iter().map(|(&to, &addr)| (addr, to)));
        assert_eq!(remotes.len(), addrs.len());
        Self { addrs, remotes }
    }
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

pub struct Dispatch {
    config: Arc<Config>,
    runtime: Handle,
    message_sender: flume::Sender<(To, To, Vec<u8>)>,
    message_receiver: flume::Receiver<(To, To, Vec<u8>)>,
    timer_sender: flume::Sender<(To, TimerId)>,
    timer_receiver: flume::Receiver<(To, TimerId)>,
    stop_sender: flume::Sender<()>,
    stop_receiver: flume::Receiver<()>,
}

impl Dispatch {
    pub fn new(config: impl Into<Arc<Config>>, runtime: Handle) -> Self {
        let (message_sender, message_receiver) = flume::unbounded();
        let (timer_sender, timer_receiver) = flume::bounded(0);
        let (shutdown_sender, shutdown_receiver) = flume::bounded(0);
        Self {
            config: config.into(),
            runtime,
            message_sender,
            message_receiver,
            timer_sender,
            timer_receiver,
            stop_sender: shutdown_sender,
            stop_receiver: shutdown_receiver,
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

impl Dispatch {
    pub fn run<M>(&self, receivers: &mut impl Receivers<Message = M>)
    where
        M: DeserializeOwned,
    {
        enum Event {
            Message(To, To, Vec<u8>),
            Timer(To, TimerId),
            Stop,
        }

        loop {
            let event = flume::Selector::new()
                .recv(&self.stop_receiver, |event| {
                    event.unwrap();
                    Event::Stop
                })
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
                Event::Stop => break,
                Event::Message(to, remote, message) => {
                    let message = bincode::options()
                        .allow_trailing_bytes()
                        .deserialize(&message)
                        .unwrap();
                    // TODO verify
                    receivers.handle(to, remote, message)
                }
                Event::Timer(to, id) => receivers.on_timer(to, super::TimerId::Tokio(id)),
            }
        }
    }
}

pub struct DispatchHandle {
    stop_sender: flume::Sender<()>,
}

impl Dispatch {
    pub fn handle(&self) -> DispatchHandle {
        DispatchHandle {
            stop_sender: self.stop_sender.clone(),
        }
    }
}

impl DispatchHandle {
    pub async fn stop(&self) {
        self.stop_sender.send_async(()).await.unwrap()
    }

    pub fn stop_sync(&self) {
        self.stop_sender.send(()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn false_alarm() {
        // let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _enter = runtime.enter();
        let config = Config::new(
            [(To::Client(0), "127.0.0.1:10000".parse().unwrap())]
                .into_iter()
                .collect(),
        );
        let dispatch = Dispatch::new(config, runtime.handle().clone());

        let mut context = dispatch.register::<()>(To::Client(0));
        let id = context.set(Duration::from_millis(10));

        let handle = dispatch.handle();
        let message_sender = dispatch.message_sender.clone();
        std::thread::spawn(move || {
            runtime.block_on(async move {
                tokio::time::sleep(Duration::from_millis(9)).await;
                message_sender
                    .send_async((
                        To::Client(0),
                        To::Replica(0),
                        bincode::options().serialize(&()).unwrap(),
                    ))
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(1)).await;
                handle.stop().await;
            });
            runtime.shutdown_background();
        });

        struct R(bool, crate::context::Context<()>, crate::context::TimerId);
        impl Receivers for R {
            type Message = ();

            fn handle(&mut self, _: To, _: To, (): Self::Message) {
                if !self.0 {
                    println!("unset");
                    self.1.unset(self.2);
                }
                self.0 = true;
            }

            fn on_timer(&mut self, _: To, _: crate::context::TimerId) {
                assert!(!self.0);
                println!("alarm");
            }
        }

        dispatch.run(&mut R(false, context, id));
    }

    #[test]
    fn false_alarm_100() {
        for _ in 0..100 {
            false_alarm()
        }
    }
}
