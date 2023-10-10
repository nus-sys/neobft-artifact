//! A context based on tokio and asynchronous IO.
//!
//! Although supported by an asynchronous reactor, protocol code, i.e.,
//! `impl Receivers` is still synchronous and running in a separated thread.

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use bincode::Options;
use rand::Rng;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{net::UdpSocket, runtime::Handle};
use tokio_util::{bytes::Bytes, sync::CancellationToken};

use crate::context::crypto::Verifier;

use super::{
    crypto::{DigestHash, Sign, Signer, Verify},
    ordered_multicast::{OrderedMulticast, Variant},
    Config, Host, OrderedMulticastReceivers, Receivers, To,
};

#[derive(Debug, Clone)]
enum Event {
    Message(Host, Host, Vec<u8>),
    LoopbackMessage(Host, Bytes),
    OrderedMulticastMessage(Host, Vec<u8>),
    Timer(Host, TimerId),
    Stop,
}

#[derive(Debug)]
pub struct Context {
    pub config: Arc<Config>,
    socket: Arc<UdpSocket>,
    runtime: Handle,
    source: Host,
    signer: Signer,
    timer_id: TimerId,
    timer_tasks: HashMap<TimerId, CancellationToken>,
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
        self.runtime.spawn(async move {
            socket
                .send_to(buf.as_ref(), addr)
                .await
                .unwrap_or_else(|err| panic!("{err} target: {addr:?}"))
        });
    }

    pub fn send_ordered_multicast(&self, message: impl Serialize + DigestHash) {
        self.send_internal(
            self.config.multicast_addr.unwrap(),
            super::ordered_multicast::serialize(&message),
        )
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
        let cancel = CancellationToken::new();
        self.timer_tasks.insert(id, cancel.clone());
        self.runtime.spawn(async move {
            loop {
                let _ = tokio::time::timeout(duration, cancel.cancelled()).await;
                if cancel.is_cancelled() {
                    return;
                }
                event.send_async(Event::Timer(source, id)).await.unwrap()
            }
        });
        id
    }

    // only works in current thread runtime
    // in multi-thread runtime, there will always be a chance that the timer task wakes concurrent
    // to this `unset` call, then this call has no way to prevent false alarm
    // TODO mutual exclusive between `Receivers` callbacks and timer tasks
    pub fn unset(&mut self, id: TimerId) {
        self.timer_tasks.remove(&id).unwrap().cancel()
    }
}

#[derive(Debug)]
pub struct Dispatch {
    config: Arc<Config>,
    runtime: Handle,
    verifier: Verifier,
    variant: Arc<Variant>,
    event: (flume::Sender<Event>, flume::Receiver<Event>),
    rdv_event: (flume::Sender<Event>, flume::Receiver<Event>),
    pub drop_rate: f64,
}

impl Dispatch {
    pub fn new(
        config: impl Into<Arc<Config>>,
        runtime: Handle,
        verify: bool,
        variant: impl Into<Arc<Variant>>,
    ) -> Self {
        let config = config.into();
        let variant = variant.into();
        let verifier = if verify {
            Verifier::new_standard(&config, variant.clone())
        } else {
            Verifier::Nop
        };
        Self {
            config,
            runtime,
            verifier,
            variant,
            event: flume::unbounded(),
            rdv_event: flume::bounded(0),
            drop_rate: 0.,
        }
    }

    pub fn register<M>(&self, receiver: Host) -> super::Context<M> {
        let socket = Arc::new(
            self.runtime
                .block_on(UdpSocket::bind(self.config.hosts[&receiver].addr))
                .unwrap_or_else(|_| panic!("binding {:?}", self.config.hosts[&receiver].addr)),
        );
        socket.set_broadcast(true).unwrap();
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
    fn run_internal<R, M, N>(&self, receivers: &mut R, into: impl Fn(OrderedMulticast<N>) -> M)
    where
        R: Receivers<Message = M>,
        M: DeserializeOwned + Verify,
        N: DeserializeOwned + DigestHash,
    {
        let deserialize = |buf: &_| {
            bincode::options()
                .allow_trailing_bytes()
                .deserialize::<M>(buf)
                .unwrap()
        };
        let mut delegate = self.variant.delegate();
        let mut pace_count = 1;
        loop {
            assert!(self.event.1.len() < 4096, "receivers overwhelmed");
            let event = flume::Selector::new()
                .recv(&self.event.1, Result::unwrap)
                .recv(&self.rdv_event.1, Result::unwrap)
                .wait();
            match event {
                Event::Stop => break,
                Event::Message(receiver, remote, message) => {
                    pace_count -= 1;
                    if self.drop_rate != 0. && rand::thread_rng().gen_bool(self.drop_rate) {
                        continue;
                    }
                    let message = deserialize(&message);
                    message.verify(&self.verifier).unwrap();
                    receivers.handle(receiver, remote, message)
                }
                Event::LoopbackMessage(receiver, message) => {
                    pace_count -= 1;
                    receivers.handle_loopback(receiver, deserialize(&message))
                }
                Event::OrderedMulticastMessage(remote, message) => {
                    pace_count -= 1;
                    if self.drop_rate != 0. && rand::thread_rng().gen_bool(self.drop_rate) {
                        continue;
                    }
                    delegate.on_receive(
                        remote,
                        self.variant.deserialize(message),
                        receivers,
                        &self.verifier,
                        &into,
                    )
                }
                Event::Timer(receiver, id) => {
                    receivers.on_timer(receiver, super::TimerId::Tokio(id))
                }
            }
            if pace_count == 0 {
                delegate.on_pace(receivers, &self.verifier, &into);
                receivers.on_pace();
                pace_count = if self.event.0.is_empty() {
                    1
                } else {
                    self.event.0.len()
                }
            }
        }
    }

    pub fn run<M>(&self, receivers: &mut impl Receivers<Message = M>)
    where
        M: DeserializeOwned + Verify,
    {
        #[derive(Deserialize)]
        enum O {}
        impl DigestHash for O {
            fn hash(&self, _: &mut impl std::hash::Hasher) {
                unreachable!()
            }
        }
        self.run_internal::<_, _, O>(receivers, |_| unimplemented!())
    }
}

#[derive(Debug)]
pub struct OrderedMulticastDispatch(Dispatch);

impl std::ops::Deref for OrderedMulticastDispatch {
    type Target = Dispatch;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Dispatch {
    pub fn enable_ordered_multicast(self) -> OrderedMulticastDispatch {
        let socket = self
            .runtime
            .block_on(UdpSocket::bind(self.config.multicast_addr.unwrap()))
            .unwrap();
        let event = self.event.0.clone();
        let config = self.config.clone();
        self.runtime.spawn(async move {
            let mut buf = vec![0; 65536];
            loop {
                let (len, remote) = socket.recv_from(&mut buf).await.unwrap();
                event
                    .try_send(Event::OrderedMulticastMessage(
                        config.remotes[&remote],
                        buf[..len].to_vec(),
                    ))
                    .unwrap()
            }
        });
        OrderedMulticastDispatch(self)
    }
}

impl OrderedMulticastDispatch {
    pub fn run<M, N>(
        &self,
        receivers: &mut (impl Receivers<Message = M> + OrderedMulticastReceivers<Message = N>),
    ) where
        M: DeserializeOwned + Verify,
        N: DeserializeOwned + DigestHash,
        OrderedMulticast<N>: Into<M>,
    {
        self.run_internal(receivers, Into::into)
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
        // let runtime = tokio::runtime::Builder::new_multi_thread()
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _enter = runtime.enter();
        let config = Config::new(
            [(Host::Replica(0), "127.0.0.1:10000".parse().unwrap())]
                .into_iter()
                .collect(),
            0,
        );
        let dispatch = Dispatch::new(
            config,
            runtime.handle().clone(),
            false,
            Variant::Unreachable,
        );

        #[derive(Serialize, Deserialize)]
        struct M;
        impl Verify for M {
            fn verify(&self, _: &Verifier) -> Result<(), crate::context::crypto::Invalid> {
                Ok(())
            }
        }

        let mut context = dispatch.register(Host::Replica(0));
        let id = context.set(Duration::from_millis(10));

        let handle = dispatch.handle();
        let event = dispatch.event.0.clone();
        std::thread::spawn(move || {
            runtime.block_on(async move {
                tokio::time::sleep(Duration::from_millis(9)).await;
                event
                    .send_async(Event::Message(
                        Host::Replica(0),
                        Host::Client(0),
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
