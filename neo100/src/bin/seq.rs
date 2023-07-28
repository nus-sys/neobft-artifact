use std::{
    net::{Ipv4Addr, UdpSocket},
    sync::Arc,
    thread::{available_parallelism, spawn},
    time::{Duration, Instant},
};

use clap::Parser;
use crossbeam::channel;
use dsys::{protocol::Generate, set_affinity, udp, Protocol};
use neo::{seq, Sequencer};
use secp256k1::SecretKey;

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    multicast: Ipv4Addr,
    #[clap(long)]
    replica_count: u8,
    #[clap(long)]
    crypto: String,
}

fn main() {
    let cli = Cli::parse();
    // sequencer's 5000 port is actually available as well
    // try to distinguish the packets that "not supposed to be received directly"
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:5001").unwrap());
    neo::init_socket(&socket, None); // only send multicast

    let mut rx = udp::Rx(socket.clone());
    let multicast_addr = (cli.multicast, 5000).into();
    match &*cli.crypto {
        "siphash" => {
            let channel = channel::unbounded();
            let mut seq = Sequencer::default()
                .then(|(buf, should_link): (_, bool)| {
                    assert!(!should_link);
                    buf
                })
                .then(channel.0);
            let seq = spawn(move || {
                set_affinity(0);
                rx.deploy(&mut seq);
            });

            let _tx = spawn(move || {
                set_affinity(1);
                channel
                    .1
                    .then(seq::SipHash {
                        multicast_addr,
                        replica_count: cli.replica_count,
                    })
                    .deploy(&mut udp::Tx::new(socket, Default::default()));
            });

            seq.join().unwrap()
        }
        "p256" => {
            let channel = channel::unbounded();
            let mut seq = Sequencer::default();
            let mut last_sign = Instant::now();
            seq.should_link = Box::new(move || {
                if Instant::now() - last_sign >= Duration::from_secs_f64(1. / (81.78 * 1000.)) {
                    last_sign = Instant::now();
                    false
                } else {
                    true
                }
            });
            let seq = spawn(move || {
                set_affinity(0);
                rx.deploy(&mut seq.then(channel.0))
            });
            for i in 1..available_parallelism().unwrap().get() - 1 {
                let mut channel = channel.1.clone();
                let socket = socket.clone();
                let _tx = spawn(move || {
                    set_affinity(i);
                    channel.deploy(
                        &mut seq::P256::new(
                            multicast_addr,
                            SecretKey::from_slice(&[b"seq", &[0; 29][..]].concat()).unwrap(),
                        )
                        .then(udp::Tx::new(socket, Default::default())),
                    )
                });
            }
            seq.join().unwrap()
        }
        _ => panic!(),
    }
}
