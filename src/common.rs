use std::time::Duration;

use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};

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

pub fn set_affinity(index: usize) {
    let mut cpu_set = CpuSet::new();
    cpu_set.set(index).unwrap();
    sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap()
}
