use std::{
    env::args,
    iter::repeat_with,
    net::ToSocketAddrs,
    process::exit,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    thread::{sleep, spawn},
    time::Duration,
};

use crossbeam::channel;
use dsys::{
    node::{Lifecycle, Workload, WorkloadMode},
    protocol::Generate,
    udp,
    unreplicated::{Client, Message},
    NodeAddr, Protocol,
};
use rand::random;

fn main() {
    let replica_ip = args().nth(1).unwrap_or(String::from("localhost"));
    let socket = Arc::new(udp::client_socket((&*replica_ip, 5000)));
    udp::init_socket(&socket);
    let mode = Arc::new(AtomicU8::new(WorkloadMode::Discard as _));
    let mut node = Workload::new_benchmark(
        Client::new(
            random(),
            NodeAddr::Socket(socket.local_addr().unwrap()),
            NodeAddr::Socket(
                (replica_ip, 5000)
                    .to_socket_addrs()
                    .unwrap()
                    .next()
                    .unwrap(),
            ),
        ),
        repeat_with::<Box<[u8]>, _>(Default::default),
        mode.clone(),
    );

    let message_channel = channel::unbounded();
    let mut rx = udp::Rx(socket.clone());
    let _rx =
        spawn(move || rx.deploy(&mut udp::Deserialize::<Message>::default().then(message_channel.0)));

    let running = Arc::new(AtomicBool::new(false));
    let node =
        spawn({
            let running = running.clone();
            // no more receiver other than the moved one
            // just keep one receiver always connected to workaround `_rx` thread
            #[allow(clippy::redundant_clone)]
            let event_channel = message_channel.1.clone();
            move || {
                Lifecycle::new(event_channel, running).deploy(&mut node.borrow_mut().each_then(
                    udp::Serialize::default().then(udp::Tx::new(socket, Default::default())),
                ));
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
