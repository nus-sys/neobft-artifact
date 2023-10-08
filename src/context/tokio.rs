//! A context based on tokio and asynchronous IO.
//!
//! Although supported by an asynchronous reactor, protocol code, i.e.,
//! `impl Receivers` is still synchronous and running in a separated thread.

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use bincode::Options;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{net::UdpSocket, runtime::Handle, task::JoinHandle};
use tokio_util::bytes::Bytes;

use crate::context::crypto::Verifier;

use super::{
    crypto::{Sign, Signer, Verify},
    Config, Host, Receivers, To,
};

#[derive(Debug, Clone)]
enum Event {
    Message(Host, Host, Vec<u8>),
    LoopbackMessage(Host, Bytes),
    Timer(Host, TimerId),
    Stop,
}

#[derive(Debug)]
pub struct Context {
    config: Arc<Config>,
    socket: Arc<UdpSocket>,
    runtime: Handle,
    source: Host,
    signer: Signer,
    timer_id: TimerId,
    timer_tasks: HashMap<TimerId, JoinHandle<()>>,
    event: flume::Sender<Event>,
    rdv_event: flume::Sender<Event>,
}

impl Context {
    pub fn send<M, N>(&self, to: To, message: N)
    where
        M: Sign<N> + Serialize,
    {
        let message = M::sign(message, &self.signer);
        let buf = Bytes::from(bincode::options().serialize(&message).unwrap());
        match to {
            To::Host(host) => self.send_internal(self.config.hosts[&host].addr, buf.clone()),
            To::AllReplica | To::AllReplicaWithLoopback => {
                for (&host, host_config) in &self.config.hosts {
                    if matches!(host, Host::Replica(_)) && host != self.source {
                        self.send_internal(host_config.addr, buf.clone())
                    }
                }
            }
            To::Loopback => {}
        }
        if matches!(to, To::Loopback | To::AllReplicaWithLoopback) {
            self.event
                .send(Event::LoopbackMessage(self.source, buf))
                .unwrap()
        }
    }

    fn send_internal(&self, addr: SocketAddr, buf: impl AsRef<[u8]> + Send + Sync + 'static) {
        let socket = self.socket.clone();
        self.runtime
            .spawn(async move { socket.send_to(buf.as_ref(), addr).await.unwrap() });
    }

    pub fn idle_hint(&self) -> bool {
        self.event.is_empty()
    }
}

pub type TimerId = u32;

impl Context {
    pub fn set(&mut self, duration: Duration) -> TimerId {
        self.timer_id += 1;
        let id = self.timer_id;
        let event = self.rdv_event.clone();
        let source = self.source;
        let task = self.runtime.spawn(async move {
            loop {
                tokio::time::sleep(duration).await;
                event.send_async(Event::Timer(source, id)).await.unwrap()
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
    event: (flume::Sender<Event>, flume::Receiver<Event>),
    rdv_event: (flume::Sender<Event>, flume::Receiver<Event>),
}

impl Dispatch {
    pub fn new(config: impl Into<Arc<Config>>, runtime: Handle, verify: bool) -> Self {
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
            event: flume::unbounded(),
            rdv_event: flume::bounded(0),
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
            event: self.event.0.clone(),
            rdv_event: self.rdv_event.0.clone(),
            timer_tasks: Default::default(),
        };
        let event = self.event.0.clone();
        let config = self.config.clone();
        self.runtime.spawn(async move {
            let mut buf = vec![0; 65536];
            loop {
                let (len, remote) = socket.recv_from(&mut buf).await.unwrap();
                event
                    .try_send(Event::Message(
                        receiver,
                        config.remotes[&remote],
                        buf[..len].to_vec(),
                    ))
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
        loop {
            let event = flume::Selector::new()
                .recv(&self.event.1, Result::unwrap)
                .recv(&self.rdv_event.1, Result::unwrap)
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
                Event::LoopbackMessage(receiver, message) => {
                    let message = bincode::options()
                        .allow_trailing_bytes()
                        .deserialize::<M>(&message)
                        .unwrap();
                    receivers.handle_loopback(receiver, message)
                }
                Event::Timer(receiver, id) => {
                    receivers.on_timer(receiver, super::TimerId::Tokio(id))
                }
            }
        }
    }
}

pub struct DispatchHandle {
    stop: Box<dyn Fn() + Send + Sync>,
    stop_async:
        Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> + Send + Sync>,
}

impl Dispatch {
    pub fn handle(&self) -> DispatchHandle {
        DispatchHandle {
            stop: Box::new({
                let rdv_event = self.rdv_event.0.clone();
                move || rdv_event.send(Event::Stop).unwrap()
            }),
            stop_async: Box::new({
                let rdv_event = self.rdv_event.0.clone();
                Box::new(move || {
                    let rdv_event = rdv_event.clone();
                    Box::pin(async move { rdv_event.send_async(Event::Stop).await.unwrap() }) as _
                })
            }),
        }
    }
}

impl DispatchHandle {
    pub fn stop(&self) {
        (self.stop)()
    }

    pub async fn stop_async(&self) {
        (self.stop_async)().await
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

        let mut context = dispatch.register(Host::Client(0));
        let id = context.set(Duration::from_millis(10));

        let handle = dispatch.handle();
        let event = dispatch.event.0.clone();
        std::thread::spawn(move || {
            runtime.block_on(async move {
                tokio::time::sleep(Duration::from_millis(9)).await;
                event
                    .send_async(Event::Message(
                        Host::Client(0),
                        Host::Replica(0),
                        bincode::options().serialize(&M).unwrap(),
                    ))
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(1)).await;
                handle.stop_async().await;
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

            fn handle_loopback(&mut self, _: Host, _: Self::Message) {
                unreachable!()
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
