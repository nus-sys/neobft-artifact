use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use crossbeam::channel;
use serde::{Deserialize, Serialize};

use crate::{
    protocol::{Composite, Generate},
    Protocol,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeAddr {
    TestClient(u32),
    TestReplica(u32),
    Socket(SocketAddr),
}

#[derive(Debug)]
pub enum NodeEvent<M> {
    Init,
    Handle(M),
    Tick,
}

#[derive(Debug)]
pub enum NodeEffect<M> {
    Send(NodeAddr, M),
    Broadcast(M),
}

pub enum ClientEvent<M> {
    Op(Box<[u8]>),
    Node(NodeEvent<M>),
}

pub enum ClientEffect<M> {
    Result(Box<[u8]>),
    Node(NodeEffect<M>),
}

pub struct Lifecycle<M> {
    message_channel: channel::Receiver<M>,
    running: Arc<AtomicBool>,
}

impl<M> Lifecycle<M> {
    pub fn new(message_channel: channel::Receiver<M>, running: Arc<AtomicBool>) -> Self {
        Self {
            message_channel,
            running,
        }
    }
}

impl<M> Generate for Lifecycle<M> {
    type Event<'a> = NodeEvent<M>;

    fn deploy<P>(&mut self, node: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>>,
    {
        assert!(!self.running.swap(true, Ordering::SeqCst));
        node.update(NodeEvent::Init);

        let tick_interval = Duration::from_millis(10);
        let mut deadline = Instant::now() + tick_interval;
        while self.running.load(Ordering::SeqCst) {
            if Instant::now() >= deadline {
                deadline = Instant::now() + tick_interval;
                node.update(NodeEvent::Tick);
            }
            match self.message_channel.recv_deadline(deadline) {
                Ok(message) => {
                    node.update(NodeEvent::Handle(message));
                }
                Err(channel::RecvTimeoutError::Disconnected) => break,
                Err(channel::RecvTimeoutError::Timeout) => {
                    deadline = Instant::now() + tick_interval;
                    node.update(NodeEvent::Tick);
                }
            }
        }
    }
}

pub struct Workload<N, I> {
    node: N,
    ops: I,
    pub results: Vec<Box<[u8]>>,
    instant: Instant,
    pub latencies: Vec<Duration>,
    pub mode: Arc<AtomicU8>,
}

pub enum WorkloadMode {
    Discard,
    Test,
    Benchmark,
}

impl WorkloadMode {
    const DISCARD: u8 = Self::Discard as _;
    const TEST: u8 = Self::Test as _;
    const BENCHMARK: u8 = Self::Benchmark as _;
}

impl<N, I> Workload<N, I> {
    pub fn new_test(node: N, ops: I) -> Self {
        Self {
            node,
            ops,
            results: Default::default(),
            instant: Instant::now(),
            latencies: Default::default(),
            mode: Arc::new(AtomicU8::new(WorkloadMode::Test as _)),
        }
    }

    pub fn new_benchmark(node: N, ops: I, mode: Arc<AtomicU8>) -> Self {
        Self {
            node,
            ops,
            results: Default::default(),
            instant: Instant::now(),
            latencies: Default::default(),
            mode,
        }
    }

    fn work<M, O>(&mut self) -> Vec<NodeEffect<M>>
    where
        N: Protocol<ClientEvent<M>>,
        N::Effect: Composite<Atom = ClientEffect<M>>,
        I: Iterator<Item = O>,
        O: Into<Box<[u8]>>,
    {
        if let Some(op) = self.ops.next() {
            self.instant = Instant::now();
            self.node.update(ClientEvent::Op(op.into())).map(|effect| {
                if let ClientEffect::Node(effect) = effect {
                    Vec::<_>::pure(effect)
                } else {
                    panic!()
                }
            })
        } else {
            Vec::<_>::NOP // record finished?
        }
    }

    fn process_effect<M, O>(&mut self, effect: ClientEffect<M>) -> Vec<NodeEffect<M>>
    where
        N: Protocol<ClientEvent<M>>,
        N::Effect: Composite<Atom = ClientEffect<M>>,
        I: Iterator<Item = O>,
        O: Into<Box<[u8]>>,
    {
        match effect {
            ClientEffect::Result(result) => {
                match self.mode.load(Ordering::SeqCst) {
                    WorkloadMode::DISCARD => {}
                    WorkloadMode::TEST => self.results.push(result),
                    WorkloadMode::BENCHMARK => self.latencies.push(Instant::now() - self.instant),
                    _ => unreachable!(),
                }
                // TODO able to throttle
                self.work()
            }
            ClientEffect::Node(effect) => Vec::<_>::pure(effect),
        }
    }
}

impl<N, I, O, M> Protocol<NodeEvent<M>> for Workload<N, I>
where
    N: Protocol<ClientEvent<M>>,
    N::Effect: Composite<Atom = ClientEffect<M>>,
    I: Iterator<Item = O>,
    O: Into<Box<[u8]>>,
{
    type Effect = Vec<NodeEffect<M>>;

    fn update(&mut self, event: NodeEvent<M>) -> Self::Effect {
        let is_init = matches!(event, NodeEvent::Init);
        let mut effect = self
            .node
            .update(ClientEvent::Node(event))
            .map(|effect| self.process_effect(effect));
        if is_init {
            effect = effect.compose(self.work());
        }
        effect
    }
}
