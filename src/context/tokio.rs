use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use bincode::Options;
use serde::Serialize;
use tokio::{net::UdpSocket, sync::mpsc::UnboundedSender, task::JoinHandle};

use super::{Signed, To};

#[derive(Debug, Clone)]
pub struct Config {
    pub addrs: HashMap<To, SocketAddr>,
    // keys
}

#[derive(Debug)]
pub struct Context {
    config: Arc<Config>,
    socket: Arc<UdpSocket>,
    source: To,
    timer_id: TimerId,
    timer_sender: UnboundedSender<(To, TimerId)>,
    timer_tasks: HashMap<TimerId, JoinHandle<()>>,
}

impl Context {
    pub fn send(&self, to: To, message: impl Serialize) {
        let buf = bincode::options().serialize(&message).unwrap();
        let config = self.config.clone();
        let socket = self.socket.clone();
        let source = self.source;
        tokio::spawn(async move {
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
                sender.send((source, id)).unwrap()
            }
        });
        self.timer_tasks.insert(id, task);
        id
    }

    pub fn unset(&mut self, id: TimerId) {
        self.timer_tasks.remove(&id).unwrap().abort()
    }
}
