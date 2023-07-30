pub mod common;
pub mod crypto;
pub mod hotstuff;
pub mod meta;
pub mod minbft;
pub mod neo;
pub mod pbft;
pub mod transport;
pub mod unreplicated;
pub mod ycsb;
pub mod zyzzyva;

pub type InvokeResult = std::pin::Pin<Box<dyn std::future::Future<Output = Vec<u8>> + Send>>;
pub trait Client {
    fn invoke(&mut self, op: &[u8]) -> InvokeResult;
}

pub trait App {
    fn replica_upcall(&mut self, op_number: meta::OpNumber, op: &[u8]) -> Vec<u8>;
    #[allow(unused_variables)]
    fn rollback_upcall(
        &mut self,
        current_number: meta::OpNumber,
        to_number: meta::OpNumber,
        ops: &[(meta::OpNumber, &[u8])],
    ) {
        unimplemented!()
    }
    #[allow(unused_variables)]
    fn commit_upcall(&mut self, op_number: meta::OpNumber) {}
}

pub mod bin {
    use std::net::Ipv4Addr;

    use serde::{Deserialize, Serialize};

    use crate::{
        meta::{Config, ReplicaId},
        transport::MulticastVariant,
    };

    /// Common configuration shared by matrix binary and control plane binary.
    ///
    // I guess there is no better place to put sharing pieces so it has to be here
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
    pub struct MatrixArgs {
        pub instance_id: String,
        pub config: Config,
        pub protocol: MatrixProtocol,
        pub app: MatrixApp,
        pub replica_id: ReplicaId,
        pub host: String,
        pub num_worker: usize,
        pub num_client: u32,
        pub drop_rate: f32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum MatrixProtocol {
        Unknown,
        UnreplicatedReplica,
        UnreplicatedClient,
        ZyzzyvaReplica {
            batch_size: usize,
        },
        ZyzzyvaClient {
            assume_byz: bool,
        },
        NeoReplica {
            variant: MulticastVariant,
            enable_vote: bool,
            batch_size: usize,
        },
        NeoClient,
        PbftReplica {
            enable_batching: bool,
        },
        PbftClient,
        HotStuffReplica,
        HotStuffClient,
        MinBFTReplica,
        MinBFTClient,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum MatrixApp {
        Unknown,
        Null,
        Ycsb,
    }

    impl Default for MatrixProtocol {
        fn default() -> Self {
            Self::Unknown
        }
    }

    impl Default for MatrixApp {
        fn default() -> Self {
            Self::Unknown
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct Spec {
        pub task: Task,
        pub replica: Vec<Node>,
        pub client: Vec<Node>,
        pub multicast: SpecMulticast,
    }

    #[derive(Debug, Clone, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub struct Task {
        pub mode: String,
        pub app: String,
        pub f: usize,
        #[serde(default)]
        pub assume_byz: bool,
        #[serde(default)]
        pub num_worker: usize,
        pub num_client: u32,
        #[serde(default)]
        pub batch_size: usize,
        #[serde(default)]
        pub enable_vote: bool,
        #[serde(default)]
        pub drop_rate: f32,
    }

    #[derive(Debug, Clone, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub struct Node {
        pub control_user: String,
        pub control_host: String,
        pub ip: Ipv4Addr,
        // pub link: String,
        // #[serde(default)]
        // pub link_speed: String,
        // pub dev_port: u8,
    }

    #[derive(Debug, Clone, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub struct SpecMulticast {
        pub ip: Ipv4Addr,
        pub variant: MulticastVariant,
        pub accel_port: Option<u8>,
    }
}
