use std::{net::SocketAddr, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    // mode
    pub client_addrs: Vec<SocketAddr>,
    pub replica_addrs: Vec<SocketAddr>,
    pub num_faulty: usize,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
    BenchmarkClient(BenchmarkClient),
    Replica(Replica),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkClient {
    pub num_group: usize,
    pub num_client: usize, // per group
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replica {
    //
    pub index: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkStats {
    pub throughput: f32,
    pub average_latency: Option<Duration>,
}
