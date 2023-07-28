use std::collections::HashMap;

use dsys::{
    node::{ClientEffect, ClientEvent},
    NodeAddr, NodeEffect, NodeEvent, Protocol,
};

use crate::{Message, Reply, Request};

pub struct Client {
    id: u32,
    addr: NodeAddr,
    multicast_addr: NodeAddr,
    request_num: u32,
    op: Option<Box<[u8]>>,
    results: HashMap<u8, Reply>,
    ticked: u32,
    f: usize,
}

impl Client {
    pub fn new(id: u32, addr: NodeAddr, multicast_addr: NodeAddr, f: usize) -> Self {
        Self {
            id,
            addr,
            multicast_addr,
            request_num: 0,
            op: None,
            results: Default::default(),
            ticked: 0,
            f,
        }
    }
}

impl Protocol<ClientEvent<Message>> for Client {
    type Effect = Option<ClientEffect<Message>>;

    fn update(&mut self, event: ClientEvent<Message>) -> Self::Effect {
        match event {
            ClientEvent::Op(op) => {
                assert!(self.op.is_none());
                self.op = Some(op);
                self.request_num += 1;
                self.ticked = 0;
                Some(self.do_request())
            }
            ClientEvent::Node(NodeEvent::Init) => None,
            ClientEvent::Node(NodeEvent::Tick) => {
                self.op.as_ref()?;
                self.ticked += 1;
                assert_ne!(self.ticked, 100);
                // if self.ticked == 1 || !self.ticked.is_power_of_two() {
                // if self.ticked == 1 {
                return None;
                // }
                // if self.ticked == 2 {
                //     eprintln!("resend");
                // }
                // Some(self.do_request())
            }
            ClientEvent::Node(NodeEvent::Handle(Message::Reply(reply))) => {
                // dbg!(&reply);
                if self.op.is_none() || reply.request_num != self.request_num {
                    return None;
                }
                self.results.insert(reply.replica_id, reply.clone());
                // TODO properly check safety
                if self.results.len() == 2 * self.f + 1 {
                    self.results.drain();
                    self.op = None;
                    Some(ClientEffect::Result(reply.result))
                } else {
                    None
                }
            }
            ClientEvent::Node(NodeEvent::Handle(_)) => unreachable!(),
        }
    }
}

impl Client {
    fn do_request(&self) -> ClientEffect<Message> {
        ClientEffect::Node(NodeEffect::Send(
            self.multicast_addr,
            Message::Request(Request {
                client_id: self.id,
                client_addr: self.addr,
                request_num: self.request_num,
                op: self.op.clone().unwrap(),
            }),
        ))
    }
}
