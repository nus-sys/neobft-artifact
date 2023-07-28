use std::{
    env::args,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    thread::{available_parallelism, spawn},
};

use crossbeam::channel;
use dsys::{protocol::Generate, set_affinity, udp, Protocol};

fn main() {
    udp::capture_interrupt();
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:5000").unwrap());
    // neo::init_socket(&socket, Some([239, 255, 1, 1].into()));
    udp::init_socket(&socket);
    let subscribers = args()
        .skip(1)
        .map(|ip| SocketAddr::new(ip.parse().unwrap(), 5000))
        .collect::<Box<_>>();

    let channel = channel::unbounded();
    let rx = spawn({
        let socket = socket.clone();
        move || {
            set_affinity(0);
            udp::Rx(socket).deploy(
                &mut (|udp::RxEvent::Receive(buf): udp::RxEvent<'_>| {
                    udp::TxEvent::Broadcast(buf.into())
                })
                .then(channel.0),
            );
        }
    });

    for i in 1..available_parallelism().unwrap().get() - 1 {
        // for i in 1..12 - 1 {
        let socket = socket.clone();
        let mut channel = channel.1.clone();
        let subscribers = subscribers.clone();
        let _tx = spawn(move || {
            set_affinity(i);
            channel.deploy(&mut udp::Tx::new(socket, subscribers))
        });
    }

    rx.join().unwrap();
}
