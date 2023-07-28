use std::{
    cmp::Ordering::{Equal, Greater, Less},
    collections::HashMap,
};

use crate::{
    meta::{ClientId, OpNumber, RequestNumber},
    App,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestApp {
    //
}

impl App for TestApp {
    fn replica_upcall(&mut self, op_number: OpNumber, op: &[u8]) -> Vec<u8> {
        [format!("[{op_number}] ").as_bytes(), op].concat()
    }
}

pub struct Reorder<M> {
    expected: u32,
    messages: HashMap<u32, M>,
}

impl<M> Reorder<M> {
    pub fn new(expected: u32) -> Self {
        Self {
            expected,
            messages: HashMap::new(),
        }
    }

    pub fn insert_reorder(&mut self, order: u32, message: M) -> Option<M> {
        assert!(order >= self.expected);
        if self.expected != order {
            // println!("* reorder");
            self.messages.insert(order, message);
            None
        } else {
            Some(message)
        }
    }

    pub fn expect_next(&mut self) -> Option<M> {
        self.expected += 1;
        self.messages.remove(&self.expected)
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct ClientTable<M>(HashMap<ClientId, (RequestNumber, Option<M>)>);
impl<M> Default for ClientTable<M> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<M: Clone> ClientTable<M> {
    pub fn insert_prepare<F: FnOnce(M)>(
        &mut self,
        id: ClientId,
        request_number: RequestNumber,
    ) -> Option<impl FnOnce(F)> {
        let reply = if let Some((saved_number, reply)) = self.0.get(&id) {
            match saved_number.cmp(&request_number) {
                Greater => None,
                Equal => reply.clone(),
                Less => {
                    self.0.insert(id, (request_number, None));
                    return None;
                }
            }
        } else {
            self.0.insert(id, (request_number, None));
            return None;
        };
        Some(|send: F| {
            if let Some(reply) = reply {
                send(reply)
            }
        })
    }

    pub fn insert_commit(&mut self, id: ClientId, request_number: RequestNumber, reply: M) {
        if let Some((saved_number, reply)) = self.0.get(&id) {
            if *saved_number > request_number {
                assert!(reply.is_none());
                return;
            }
        }
        self.0.insert(id, (request_number, Some(reply)));
    }
}
