//! A context based on tokio and asynchronous IO.
//!
//! Although the event management is asynchronous, protocol code, i.e.,
//! `impl Receivers` is still synchronous and running in a separated thread.

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use bincode::Options;
use hmac::{Hmac, Mac};
use k256::{
    ecdsa::{
        signature::{DigestSigner, DigestVerifier},
        SigningKey,
    },
    sha2::{Digest, Sha256},
};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{net::UdpSocket, runtime::Handle, task::JoinHandle};

use crate::context::Hasher;

use super::{DigestHash, Receivers, ReplicaIndex, Signed, To};

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub hosts: HashMap<To, ConfigHost>,
    pub remotes: HashMap<SocketAddr, To>,
}

#[derive(Debug, Clone)]
pub struct ConfigHost {
    pub addr: SocketAddr,
    pub signing_key: Option<SigningKey>,
    pub hmac_hasher: Hmac<Sha256>,
}

impl Config {
    pub fn new(addrs: HashMap<To, SocketAddr>) -> Self {
        let remotes = HashMap::from_iter(addrs.iter().map(|(&to, &addr)| (addr, to)));
        assert_eq!(remotes.len(), addrs.len());
        let hosts = HashMap::from_iter(addrs.into_iter().map(|(to, addr)| {
            (
                to,
                ConfigHost {
                    addr,
                    signing_key: match to {
                        To::Client(_) => None,
                        To::Replica(index) => Some(Self::k256(index)),
                        To::AllReplica => unimplemented!(),
                    },
                    hmac_hasher: Self::hmac(to),
                },
            )
        }));
        Self { hosts, remotes }
    }

    fn hmac(to: To) -> Hmac<Sha256> {
        match to {
            To::Client(index) => {
                Hmac::new_from_slice(format!("client-{index}").as_bytes()).unwrap()
            }
            To::Replica(index) => {
                Hmac::new_from_slice(format!("replica-{index}").as_bytes()).unwrap()
            }
            To::AllReplica => unimplemented!(),
        }
    }

    fn k256(index: ReplicaIndex) -> SigningKey {
        let k = format!("replica-{index}");
        let mut buf = [0; 32];
        buf[..k.as_bytes().len()].copy_from_slice(k.as_bytes());
        SigningKey::from_slice(&buf).unwrap()
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
    pub fn send(&self, to: To, message: impl Serialize + DigestHash) {
        // really?
        assert!(matches!(self.source, To::Client(_)) || matches!(to, To::Client(_)));
        let mut hasher = Hasher::HMac(self.config.hosts[&self.source].hmac_hasher.clone());
        message.hash(&mut hasher);
        let Hasher::HMac(hasher) = hasher else {
            unreachable!()
        };
        self.send_internal(
            to,
            bincode::options()
                .serialize(&Signed::Hmac(
                    message,
                    hasher.finalize().into_bytes().into(),
                ))
                .unwrap(),
        )
    }

    pub fn send_signed<M, N, F: FnOnce(&mut N, Signed<M>)>(
        &self,
        to: To,
        message: M,
        loopback: impl Into<Option<F>>,
    ) where
        M: Serialize + DigestHash,
    {
        assert!(matches!(self.source, To::Replica(_)));
        assert!(matches!(to, To::Replica(_) | To::AllReplica));
        let mut hasher = Hasher::Sha256(Sha256::new());
        message.hash(&mut hasher);
        let Hasher::Sha256(hasher) = hasher else {
            unreachable!()
        };
        let signature = self.config.hosts[&self.source]
            .signing_key
            .as_ref()
            .unwrap()
            .sign_digest(hasher);
        let message = Signed::K256(message, signature);
        self.send_internal(to, bincode::options().serialize(&message).unwrap());
        if let Some(loopback) = loopback.into() {
            // loopback(message)
        }
    }

    fn send_internal(&self, to: To, buf: Vec<u8>) {
        let config = self.config.clone();
        let socket = self.socket.clone();
        let source = self.source;
        self.runtime.spawn(async move {
            match to {
                To::Client(_) | To::Replica(_) => {
                    assert_ne!(to, source);
                    socket.send_to(&buf, config.hosts[&to].addr).await.unwrap();
                }
                To::AllReplica => {
                    for (&to, host) in &config.hosts {
                        if matches!(to, To::Replica(_)) && to != source {
                            socket.send_to(&buf, host.addr).await.unwrap();
                        }
                    }
                }
            }
        });
    }
}

pub type TimerId = u32;

impl Context {
    pub fn set(&mut self, duration: Duration) -> TimerId {
        self.timer_id += 1;
        let id = self.timer_id;
        let sender = self.timer_sender.clone();
        let source = self.source;
        let task = self.runtime.spawn(async move {
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
                .block_on(UdpSocket::bind(self.config.hosts[&receiver].addr))
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
        M: DeserializeOwned + DigestHash,
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
                        .deserialize::<Signed<M>>(&message)
                        .unwrap();
                    let message = match message {
                        Signed::Plain(_) => unreachable!(),
                        Signed::K256(message, signature) => {
                            let mut hasher = Hasher::Sha256(Sha256::new());
                            message.hash(&mut hasher);
                            let Hasher::Sha256(hasher) = hasher else {
                                unreachable!()
                            };
                            self.config.hosts[&remote]
                                .signing_key
                                .as_ref()
                                .unwrap()
                                .verifying_key()
                                .verify_digest(hasher, &signature)
                                .unwrap();
                            message
                        }
                        Signed::Hmac(message, mac) => {
                            let mut hasher =
                                Hasher::HMac(self.config.hosts[&remote].hmac_hasher.clone());
                            message.hash(&mut hasher);
                            let Hasher::HMac(hasher) = hasher else {
                                unreachable!()
                            };
                            hasher.verify(&mac.into()).unwrap();
                            message
                        }
                    };
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
    use serde::Deserialize;

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

        #[derive(Serialize, Deserialize)]
        struct M;
        impl DigestHash for M {
            fn hash(&self, _: &mut Hasher) {}
        }

        let mut context = dispatch.register::<M>(To::Client(0));
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
                        bincode::options().serialize(&M).unwrap(),
                    ))
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(1)).await;
                handle.stop().await;
            });
            runtime.shutdown_background();
        });

        struct R(bool, crate::context::Context<M>, crate::context::TimerId);
        impl Receivers for R {
            type Message = M;

            fn handle(&mut self, _: To, _: To, M: Self::Message) {
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
