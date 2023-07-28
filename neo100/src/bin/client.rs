use std::{
    iter::repeat_with,
    net::{IpAddr, ToSocketAddrs},
    process::exit,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    thread::{sleep, spawn},
    time::Duration,
};

use clap::Parser;
use crossbeam::channel;
use dsys::{
    node::{Lifecycle, Workload, WorkloadMode},
    protocol::Generate,
    udp, NodeAddr, Protocol,
};
use neo::{Client, RxP256Event};
use rand::random;

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    seq_ip: IpAddr,
    #[clap(short)]
    f: usize,
}

fn main() {
    let cli = Cli::parse();
    let socket = Arc::new(udp::client_socket((cli.seq_ip, 5001)));
    neo::init_socket(&socket, None);
    let mode = Arc::new(AtomicU8::new(WorkloadMode::Discard as _));
    let mut node = Workload::new_benchmark(
        Client::new(
            random(),
            NodeAddr::Socket(socket.local_addr().unwrap()),
            NodeAddr::Socket(
                (cli.seq_ip, 5001)
                    .to_socket_addrs()
                    .unwrap()
                    .next()
                    .unwrap(),
            ),
            cli.f,
        ),
        repeat_with::<Box<[u8]>, _>(Default::default),
        mode.clone(),
    );

    // udp::Rx -> neo::Rx::Reject -> (<unreachable>, RxP256 -> _msg_)
    // RxP256 here is trivial: client bound replies are not signed
    let message_channel = channel::unbounded();
    let mut rx = udp::Rx(socket.clone());
    let _rx = spawn(move || {
        rx.deploy(
            &mut neo::Rx::UnicastOnly
                .then((
                    |_| unreachable!(), // receive multicast
                    (|event| {
                        if let RxP256Event::Unicast(message) = event {
                            message
                        } else {
                            unreachable!()
                        }
                    })
                    .then(message_channel.0),
                ))
                .then(Into::into),
        )
    });

    // _msg_ ~> Lifecycle -> `node` --> neo::Tx -> udp::Tx
    let running = Arc::new(AtomicBool::new(false));
    let node = spawn({
        let running = running.clone();
        // no more receiver needed other than the moved one
        // just keep one receiver always connected to workaround `_rx` thread
        #[allow(clippy::redundant_clone)]
        let event_channel = message_channel.1.clone();
        move || {
            Lifecycle::new(event_channel, running).deploy(
                &mut node.borrow_mut().each_then(
                    neo::Tx {
                        multicast: Some((cli.seq_ip, 5001).into()),
                    }
                    .then(udp::Tx::new(socket, Default::default())),
                ),
            );
            node
        }
    });

    sleep(Duration::from_secs(2)); // warm up
    mode.store(WorkloadMode::Benchmark as _, Ordering::SeqCst);
    sleep(Duration::from_secs(10));
    mode.store(WorkloadMode::Discard as _, Ordering::SeqCst);
    sleep(Duration::from_secs(2)); // cool down

    running.store(false, Ordering::SeqCst);
    let mut latencies = node.join().unwrap().latencies;
    println!("{}", latencies.len());
    if latencies.is_empty() {
        exit(1)
    } else {
        latencies.sort_unstable();
        println!(
            "50th {:?} 99th {:?}",
            latencies[latencies.len() / 2],
            latencies[latencies.len() * 99 / 100]
        )
    }
}
