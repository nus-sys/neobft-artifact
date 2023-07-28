use std::{
    convert::identity,
    net::UdpSocket,
    sync::Arc,
    thread::{available_parallelism, spawn},
};

use crossbeam::channel;
use dsys::{
    app,
    node::Lifecycle,
    protocol::Generate,
    set_affinity, udp,
    unreplicated::{Message, Replica},
    App, Protocol,
};

fn main() {
    udp::capture_interrupt();

    let socket = Arc::new(UdpSocket::bind("0.0.0.0:5000").unwrap());
    udp::init_socket(&socket);
    let node = Replica::new(App::Null(app::Null));

    let message_channel = channel::unbounded();
    let mut rx = udp::Rx(socket.clone());
    let rx = spawn(move || {
        set_affinity(0);
        rx.deploy(&mut udp::Deserialize::<Message>::default().then(message_channel.0))
    });

    let effect_channel = channel::unbounded();
    let _node = spawn(move || {
        set_affinity(1);
        Lifecycle::new(message_channel.1, Default::default())
            .deploy(&mut node.then(effect_channel.0))
    });

    // save the last parallelism for IRQ handling
    for i in 2..available_parallelism().unwrap().get() - 1 {
        // for i in 2..8 - 1 {
        let mut effect_channel = effect_channel.1.clone();
        let socket = socket.clone();
        let _tx = spawn(move || {
            set_affinity(i);
            effect_channel.deploy(
                &mut identity
                    .then(udp::Serialize::default().then(udp::Tx::new(socket, Default::default()))),
            )
        });
    }

    rx.join().unwrap();
}
