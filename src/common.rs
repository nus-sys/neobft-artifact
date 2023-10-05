use std::time::Duration;

use crate::context::{Context, TimerId};

#[derive(Debug)]
pub struct Timer {
    pub id: Option<TimerId>,
    duration: Duration,
}

impl Timer {
    pub fn new(duration: Duration) -> Self {
        Self { id: None, duration }
    }

    pub fn set<M>(&mut self, context: &mut Context<M>) {
        let evicted = self.id.replace(context.set(self.duration));
        assert!(evicted.is_none())
    }

    pub fn unset<M>(&mut self, context: &mut Context<M>) {
        context.unset(self.id.take().unwrap())
    }

    pub fn reset<M>(&mut self, context: &mut Context<M>) {
        self.unset(context);
        self.set(context)
    }
}
