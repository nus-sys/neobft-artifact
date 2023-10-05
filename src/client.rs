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

impl<T: OnResult + 'static> From<T> for Box<dyn OnResult> {
    fn from(value: T) -> Self {
        Box::new(value)
    }
}

pub trait Client {
    type Message;

    fn invoke(&self, op: Vec<u8>, on_result: impl Into<Box<dyn OnResult>>);

    fn handle(&self, message: Self::Message);

    // on timer
}

pub struct Benchmark<C> {
    clients: HashMap<To, Arc<C>>,
    finish_sender: flume::Sender<To>,
    finish_receiver: flume::Receiver<To>,
    invoke_starts: HashMap<To, Instant>,
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
            invoke_starts: Default::default(),
            latencies: Default::default(),
        }
    }

    pub fn insert_client(&mut self, to: To, client: C) {
        let evicted = self.clients.insert(to, Arc::new(client));
        assert!(evicted.is_none())
    }

    pub fn close_loop(&mut self, duration: Duration)
    where
        C: Client,
    {
        for (&index, client) in &self.clients {
            let finish_sender = self.finish_sender.clone();
            self.invoke_starts.insert(index, Instant::now());
            // TODO
            client.invoke(Default::default(), move |_| {
                finish_sender.send(index).unwrap()
            });
        }
        let deadline = Instant::now() + duration;
        while let Ok(index) = self.finish_receiver.recv_deadline(deadline) {
            self.latencies
                .push(self.invoke_starts.remove(&index).unwrap().elapsed());

            let finish_sender = self.finish_sender.clone();
            self.invoke_starts.insert(index, Instant::now());
            // TODO
            self.clients[&index].invoke(Default::default(), move |_| {
                finish_sender.send(index).unwrap()
            });
        }
    }

    pub fn run(&self) -> impl FnOnce(&mut crate::context::tokio::Runtime)
    where
        C: Client + 'static,
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
