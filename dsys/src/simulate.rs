use std::collections::{HashMap, VecDeque};

use crate::{protocol::Composite, NodeAddr, NodeEffect, NodeEvent, Protocol};

pub struct Simulate<N, M> {
    pub nodes: HashMap<NodeAddr, N>,
    messages: VecDeque<(NodeAddr, M)>,
    tick_count: HashMap<NodeAddr, u32>,
}

impl<N, M> Default for Simulate<N, M> {
    fn default() -> Self {
        Self {
            nodes: Default::default(),
            tick_count: Default::default(),
            messages: Default::default(),
        }
    }
}

impl<N, M> Simulate<N, M> {
    pub fn init(&mut self)
    where
        N: Protocol<NodeEvent<M>>,
        N::Effect: Composite<Atom = NodeEffect<M>>,
    {
        for effect in self
            .nodes
            .values_mut()
            .map(|node| node.update(NodeEvent::Init))
            .collect::<Vec<_>>()
        {
            self.push_effect(effect)
        }
    }

    pub fn progress(&mut self) -> bool
    where
        N: Protocol<NodeEvent<M>>,
        N::Effect: Composite<Atom = NodeEffect<M>>,
    {
        let Some((destination, message)) = self.messages.pop_front() else {
            return false;
        };
        let effect = self
            .nodes
            .get_mut(&destination)
            .unwrap()
            .update(NodeEvent::Handle(message));
        self.push_effect(effect);
        true
    }

    pub fn tick(&mut self, addr: NodeAddr)
    where
        N: Protocol<NodeEvent<M>>,
        N::Effect: Composite<Atom = NodeEffect<M>>,
    {
        *self.tick_count.entry(addr).or_default() += 1;
        let effect = self.nodes.get_mut(&addr).unwrap().update(NodeEvent::Tick);
        self.push_effect(effect);
    }

    fn push_effect(&mut self, mut effect: impl Composite<Atom = NodeEffect<M>>) {
        while let Some(basic_effect) = effect.decompose() {
            match basic_effect {
                NodeEffect::Send(address, message) => self.messages.push_back((address, message)),
                NodeEffect::Broadcast(_) => todo!(),
            }
        }
    }
}
