use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::de::DeserializeOwned;

use crate::context::To;

pub trait OnResult {
    fn apply(self: Box<Self>, result: Vec<u8>);
}

impl<F: FnOnce(Vec<u8>)> OnResult for F {
    fn apply(self: Box<Self>, result: Vec<u8>) {
        self(result)
    }
}

impl<T: OnResult + Send + Sync + 'static> From<T> for Box<dyn OnResult + Send + Sync> {
    fn from(value: T) -> Self {
        Box::new(value)
    }
}

pub trait Client {
    type Message;

    fn invoke(&self, op: Vec<u8>, on_result: impl Into<Box<dyn OnResult + Send + Sync>>);

    fn handle(&self, message: Self::Message);

    // on timer
}

pub struct Benchmark<C> {
    clients: HashMap<To, Arc<C>>,
    finish_sender: flume::Sender<(To, Duration)>,
    finish_receiver: flume::Receiver<(To, Duration)>,
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
            finish_sender,
            finish_receiver,
            latencies: Default::default(),
        }
    }

    pub fn insert_client(&mut self, to: To, client: C) {
        let evicted = self.clients.insert(to, Arc::new(client));
        assert!(evicted.is_none())
    }

    pub fn close_loop(&mut self, duration: Duration, bootstrap: bool)
    where
        C: Client,
    {
        if bootstrap {
            for (&index, client) in &self.clients {
                let finish_sender = self.finish_sender.clone();
                let start = Instant::now();
                // TODO
                client.invoke(Default::default(), move |_| {
                    finish_sender.send((index, start.elapsed())).unwrap()
                });
            }
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
        C::Message: DeserializeOwned,
    {
        struct R<C>(HashMap<To, Arc<C>>);
        impl<C> crate::context::Receivers for R<C>
        where
            C: Client,
        {
            type Message = C::Message;

            fn handle(&mut self, to: To, _: To, message: Self::Message) {
                self.0[&to].handle(message)
            }

            fn on_timer(&mut self, to: To, _: crate::context::TimerId) {
                panic!("{to:?} timeout")
            }
        }

        let mut receivers = R(self.clients.clone());
        move |runtime| runtime.run(&mut receivers)
    }
}
