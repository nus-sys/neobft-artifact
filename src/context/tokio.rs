//! A context based on tokio and asynchronous IO.
//!
//! Although supported by an asynchronous reactor, protocol code, i.e.,
//! `impl Receivers` is still synchronous and running in a separated thread.

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use bincode::Options;
use hmac::{Hmac, Mac};
use k256::{ecdsa::SigningKey, sha2::Sha256};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{net::UdpSocket, runtime::Handle, task::JoinHandle};

use crate::context::crypto::Verifier;

use super::{
    crypto::{Sign, Signer, Verify},
    Host, Receivers, ReplicaIndex, To,
};

#[derive(Debug, Clone)]
pub struct Config {
    pub hosts: HashMap<Host, ConfigHost>,
    pub remotes: HashMap<SocketAddr, Host>,
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

#[derive(Debug)]
pub struct Context {
    config: Arc<Config>,
    socket: Arc<UdpSocket>,
    runtime: Handle,
    source: Host,
    signer: Signer,
    timer_id: TimerId,
    timer_sender: flume::Sender<(Host, TimerId)>,
    timer_tasks: HashMap<TimerId, JoinHandle<()>>,
}

impl Context {
    pub fn send<M, N>(&self, to: To, message: N)
    where
        M: Sign<N> + Serialize,
    {
        let message = M::sign(message, &self.signer);
        self.send_buf(to, bincode::options().serialize(&message).unwrap())
    }

    pub fn send_buf(&self, to: To, buf: Vec<u8>) {
        let config = self.config.clone();
        let socket = self.socket.clone();
        let source = self.source;
        self.runtime.spawn(async move {
            match to {
                To::Host(host) => {
                    assert_ne!(host, source);
                    socket
                        .send_to(&buf, config.hosts[&host].addr)
                        .await
                        .unwrap();
                }
                To::AllReplica => {
                    for (&host, host_config) in &config.hosts {
                        if matches!(host, Host::Replica(_)) && host != source {
                            socket.send_to(&buf, host_config.addr).await.unwrap();
                        }
                    }
                }
                _ => todo!(),
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
    verifier: Verifier,
    message_sender: flume::Sender<(Host, Host, Vec<u8>)>,
    message_receiver: flume::Receiver<(Host, Host, Vec<u8>)>,
    timer_sender: flume::Sender<(Host, TimerId)>,
    timer_receiver: flume::Receiver<(Host, TimerId)>,
    stop_sender: flume::Sender<()>,
    stop_receiver: flume::Receiver<()>,
}

impl Dispatch {
    pub fn new(config: impl Into<Arc<Config>>, runtime: Handle, verify: bool) -> Self {
        let (message_sender, message_receiver) = flume::unbounded();
        let (timer_sender, timer_receiver) = flume::bounded(0);
        let (shutdown_sender, shutdown_receiver) = flume::bounded(0);
        let config = config.into();
        let verifier = if verify {
            Verifier::new_standard(&config)
        } else {
            Verifier::Nop
        };
        Self {
            config,
            runtime,
            verifier,
            message_sender,
            message_receiver,
            timer_sender,
            timer_receiver,
            stop_sender: shutdown_sender,
            stop_receiver: shutdown_receiver,
        }
    }

    pub fn register<M>(&self, receiver: Host) -> super::Context<M> {
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
            signer: Signer {
                signing_key: self.config.hosts[&receiver].signing_key.clone(),
                hmac: self.config.hmac.clone(),
            },
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
        M: DeserializeOwned + Verify,
    {
        enum Event {
            Message(Host, Host, Vec<u8>),
            Timer(Host, TimerId),
            Stop,
        }

        loop {
            let event = flume::Selector::new()
                .recv(&self.stop_receiver, |event| {
                    event.unwrap();
                    Event::Stop
                })
                .recv(&self.message_receiver, |event| {
                    let (receiver, remote, message) = event.unwrap();
                    Event::Message(receiver, remote, message)
                })
                .recv(&self.timer_receiver, |event| {
                    let (receiver, id) = event.unwrap();
                    Event::Timer(receiver, id)
                })
                .wait();
            match event {
                Event::Stop => break,
                Event::Message(receiver, remote, message) => {
                    let message = bincode::options()
                        .allow_trailing_bytes()
                        .deserialize::<M>(&message)
                        .unwrap();
                    message.verify(&self.verifier).unwrap();
                    receivers.handle(receiver, remote, message)
                }
                Event::Timer(receiver, id) => {
                    receivers.on_timer(receiver, super::TimerId::Tokio(id))
                }
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
            [(Host::Client(0), "127.0.0.1:10000".parse().unwrap())]
                .into_iter()
                .collect(),
        );
        let dispatch = Dispatch::new(config, runtime.handle().clone(), false);

        #[derive(Serialize, Deserialize)]
        struct M;
        impl Verify for M {
            fn verify(&self, _: &Verifier) -> Result<(), crate::context::crypto::Invalid> {
                Ok(())
            }
        }

        let mut context = dispatch.register::<M>(Host::Client(0));
        let id = context.set(Duration::from_millis(10));

        let handle = dispatch.handle();
        let message_sender = dispatch.message_sender.clone();
        std::thread::spawn(move || {
            runtime.block_on(async move {
                tokio::time::sleep(Duration::from_millis(9)).await;
                message_sender
                    .send_async((
                        Host::Client(0),
                        Host::Replica(0),
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

            fn handle(&mut self, _: Host, _: Host, M: Self::Message) {
                if !self.0 {
                    println!("unset");
                    self.1.unset(self.2);
                }
                self.0 = true;
            }

            fn on_timer(&mut self, _: Host, _: crate::context::TimerId) {
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
