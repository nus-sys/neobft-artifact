pub mod app;
pub mod crypto;
pub mod node;
pub mod protocol;
pub mod simulate;
pub mod udp;
pub mod unreplicated;

pub use crate::app::App;
pub use crate::node::{NodeAddr, NodeEffect, NodeEvent};
pub use crate::protocol::Protocol;
pub use crate::simulate::Simulate;

pub fn set_affinity(affinity: usize) {
    use nix::{
        sched::{sched_setaffinity, CpuSet},
        unistd::Pid,
    };
    let mut cpu_set = CpuSet::new();
    cpu_set.set(affinity).unwrap();
    sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
}
