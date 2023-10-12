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
    app::Workload,
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

    fn abort(&self) -> Option<BoxedConsume> {
        unimplemented!()
    }

    fn handle(&self, message: Self::Message);

    // on timer
}

impl<T: Client> Client for Arc<T> {
    type Message = T::Message;

    fn invoke(&self, op: Vec<u8>, consume: impl Into<BoxedConsume>) {
        T::invoke(self, op, consume)
    }

    fn abort(&self) -> Option<BoxedConsume> {
        T::abort(self)
    }

    fn handle(&self, message: Self::Message) {
        T::handle(self, message)
    }
}

#[derive(Debug)]
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

    pub fn close_loop(
        &mut self,
        duration: Duration,
        workload: &Workload,
        runtime: tokio::runtime::Handle,
    ) where
        C: Client + Send + Sync + 'static,
    {
        let invoke = |index, client: Arc<C>| {
            let txn = workload.generate(client.clone(), &mut rand::thread_rng());
            let finish_sender = self.finish_sender.clone();
            async move {
                let start = Instant::now();
                txn.await;
                finish_sender.send((index, start.elapsed())).unwrap()
            }
        };

        if self.bootstrap {
            for (i, (&index, client)) in self.clients.iter().enumerate() {
                // synchronously finish the first invocation, to avoid first-packet reordering
                if i == 0 {
                    runtime.block_on(invoke(index, client.clone()))
                } else {
                    runtime.spawn(invoke(index, client.clone()));
                }
            }
            self.bootstrap = false;
        }
        let deadline = Instant::now() + duration;
        while let Ok((index, latency)) = self.finish_receiver.recv_deadline(deadline) {
            self.latencies.push(latency);
            runtime.spawn(invoke(index, self.clients[&index].clone()));
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

#[derive(Debug)]
pub struct RunBenchmarkConfig {
    pub dispatch_config: Config,
    pub offset: usize,
    pub num_group: usize,
    pub num_client: usize,
    pub duration: Duration,
    pub workload: Workload,
}

pub fn run_benchmark<C>(
    config: RunBenchmarkConfig,
    new_client: impl Fn(Context<C::Message>, ClientIndex) -> C,
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
    
    // println!("{config:?}");
    let barrier = Arc::new(Barrier::new(config.num_group));
    let dispatch_config = Arc::new(config.dispatch_config);
    let groups = Vec::from_iter(
        repeat((barrier, Arc::new(config.workload)))
            .take(config.num_group)
            .enumerate()
            .map(|(group_index, (barrier, workload))| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let handle = runtime.handle().clone();
                let mut dispatch = Dispatch::new(
                    dispatch_config.clone(),
                    handle.clone(),
                    false,
                    Variant::Unreachable,
                );

                let mut benchmark = Benchmark::new();
                for group_offset in 0..config.num_client {
                    let index = (config.offset + group_index * config.num_client + group_offset)
                        as ClientIndex;
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
                    if group_index == 0 {
                        benchmark.close_loop(Duration::from_secs(1), &workload, handle.clone());
                    }
                    barrier.wait();
                    benchmark.close_loop(Duration::from_secs(1), &workload, handle.clone());
                    benchmark.latencies.clear();
                    benchmark.close_loop(config.duration, &workload, handle);
                    benchmark
                });

                Group {
                    benchmark_thread,
                    runtime_thread,
                    dispatch_thread,
                    dispatch_handle,
                }
            }),
    );

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
