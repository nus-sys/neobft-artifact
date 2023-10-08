use std::{
    collections::HashMap,
    iter::repeat,
    sync::{Arc, Barrier},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;

use crate::{
    common::set_affinity,
    context::{
        crypto::Verify,
        ordered_multicast::Variant,
        tokio::{Dispatch, DispatchHandle},
        ClientIndex, Config, Host,
    },
    Context,
};

pub trait OnResult {
    fn apply(self: Box<Self>, result: Vec<u8>);
}

impl<F: FnOnce(Vec<u8>)> OnResult for F {
    fn apply(self: Box<Self>, result: Vec<u8>) {
        self(result)
    }
}

pub type BoxedConsume = Box<dyn OnResult + Send + Sync>;

impl<T: OnResult + Send + Sync + 'static> From<T> for BoxedConsume {
    fn from(value: T) -> Self {
        Box::new(value)
    }
}

impl std::fmt::Debug for BoxedConsume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BoxedConsume").field(&"..").finish()
    }
}

pub trait Client {
    type Message;

    fn invoke(&self, op: Vec<u8>, consume: impl Into<BoxedConsume>);

    fn handle(&self, message: Self::Message);

    // on timer
}

pub struct Benchmark<C> {
    clients: HashMap<Host, Arc<C>>,
    bootstrap: bool,
    finish_sender: flume::Sender<(Host, Duration)>,
    finish_receiver: flume::Receiver<(Host, Duration)>,
    pub latencies: Vec<Duration>,
}

impl<C> Default for Benchmark<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> Benchmark<C> {
    pub fn new() -> Self {
        let (finish_sender, finish_receiver) = flume::unbounded();
        Self {
            clients: Default::default(),
            bootstrap: true,
            finish_sender,
            finish_receiver,
            latencies: Default::default(),
        }
    }

    pub fn insert_client(&mut self, index: ClientIndex, client: C) {
        let evicted = self.clients.insert(Host::Client(index), Arc::new(client));
        assert!(evicted.is_none())
    }

    pub fn close_loop(&mut self, duration: Duration)
    where
        C: Client,
    {
        if self.bootstrap {
            for (&index, client) in &self.clients {
                let finish_sender = self.finish_sender.clone();
                let start = Instant::now();
                // TODO
                client.invoke(Default::default(), move |_| {
                    finish_sender.send((index, start.elapsed())).unwrap()
                });
            }
            self.bootstrap = false;
        }
        let deadline = Instant::now() + duration;
        while let Ok((index, latency)) = self.finish_receiver.recv_deadline(deadline) {
            self.latencies.push(latency);

            let finish_sender = self.finish_sender.clone();
            let start = Instant::now();
            // TODO
            self.clients[&index].invoke(Default::default(), move |_| {
                finish_sender.send((index, start.elapsed())).unwrap()
            });
        }
    }

    pub fn run_dispatch(&self) -> impl FnOnce(&mut crate::context::tokio::Dispatch) + Send
    where
        C: Client + Send + Sync + 'static,
        C::Message: DeserializeOwned + Verify,
    {
        struct R<C>(HashMap<Host, Arc<C>>);
        impl<C> crate::context::Receivers for R<C>
        where
            C: Client,
        {
            type Message = C::Message;

            fn handle(&mut self, receiver: Host, _: Host, message: Self::Message) {
                self.0[&receiver].handle(message)
            }

            fn on_timer(&mut self, receiver: Host, _: crate::context::TimerId) {
                panic!("{receiver:?} timeout")
            }
        }

        let mut receivers = R(self.clients.clone());
        move |runtime| runtime.run(&mut receivers)
    }
}

pub fn run_benchmark<C>(
    dispatch_config: Config,
    new_client: impl Fn(Context<C::Message>, ClientIndex) -> C,
    num_group: usize,
    num_client: usize,
    duration: Duration,
) -> Vec<Duration>
where
    C: Client + Send + Sync + 'static,
    C::Message: DeserializeOwned + Verify,
{
    struct Group<C> {
        benchmark_thread: JoinHandle<Benchmark<C>>,
        runtime_thread: JoinHandle<()>,
        dispatch_thread: JoinHandle<()>,
        dispatch_handle: DispatchHandle,
    }

    let barrier = Arc::new(Barrier::new(num_group));
    let dispatch_config = Arc::new(dispatch_config);
    let groups = Vec::from_iter(repeat(barrier).take(num_group).enumerate().map(
        |(group_index, barrier)| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let mut dispatch = Dispatch::new(
                dispatch_config.clone(),
                runtime.handle().clone(),
                false,
                Variant::Unreachable,
            );

            let mut benchmark = Benchmark::new();
            for group_offset in 0..num_client {
                let index = (group_index * num_client + group_offset) as ClientIndex;
                let client = new_client(dispatch.register(Host::Client(index)), index);
                benchmark.insert_client(index, client);
            }

            let cancel = CancellationToken::new();
            let runtime_thread = std::thread::spawn({
                set_affinity(group_index * 3);
                let cancel = cancel.clone();
                move || runtime.block_on(cancel.cancelled())
            });

            let dispatch_handle = dispatch.handle();
            let run = benchmark.run_dispatch();
            let dispatch_thread = std::thread::spawn(move || {
                set_affinity(group_index * 3 + 1);
                run(&mut dispatch);
                cancel.cancel()
            });

            let benchmark_thread = std::thread::spawn(move || {
                set_affinity(group_index * 3 + 2);
                barrier.wait();
                benchmark.close_loop(Duration::from_secs(3));
                benchmark.latencies.clear();
                benchmark.close_loop(duration);
                benchmark
            });

            Group {
                benchmark_thread,
                runtime_thread,
                dispatch_thread,
                dispatch_handle,
            }
        },
    ));

    let mut latencies = Vec::new();
    for group in groups {
        let benchmark = group.benchmark_thread.join().unwrap();
        latencies.extend(benchmark.latencies);
        group.dispatch_handle.stop();
        group.dispatch_thread.join().unwrap();
        group.runtime_thread.join().unwrap();
    }
    latencies
}
