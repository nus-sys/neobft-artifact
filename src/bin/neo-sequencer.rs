use std::{
    env::args,
    iter::repeat,
    net::{Ipv4Addr, UdpSocket},
    sync::Arc,
    thread::{available_parallelism, spawn},
};

use permissioned_blockchain::{common::set_affinity, context::ordered_multicast::Sequencer};

fn main() {
    let ip = args().nth(1).unwrap().parse::<Ipv4Addr>().unwrap();
    let mut sequencer = match args().nth(2).as_deref() {
        Some("half-sip-hash") => Sequencer::new_half_sip_hash(),
        Some("k256") => Sequencer::new_k256(),
        _ => unimplemented!(),
    };
    let multicast_ip = args().nth(3).unwrap().parse::<Ipv4Addr>().unwrap();

    let socket = Arc::new(UdpSocket::bind((ip, 60004)).unwrap());
    let messages = flume::bounded::<(Vec<_>, _)>(1024);

    // this has to go first or compiler cannot guess `messages` type
    let mut run = || {
        set_affinity(0);
        let mut buf = vec![0; 65536];
        loop {
            let (len, _) = socket.recv_from(&mut buf).unwrap();
            sequencer.process(&mut buf[..len]);
            messages
                .0
                .send((buf[..len].to_vec(), sequencer.postprocess()))
                .unwrap()
        }
    };

    for ((index, messages), socket) in repeat(messages.1)
        .take(usize::from(available_parallelism().unwrap()) - 1)
        .enumerate()
        .zip(repeat(socket.clone()))
    {
        spawn(move || {
            set_affinity(index + 1);
            loop {
                let (mut buf, postprocess) = messages.recv().unwrap();
                postprocess(&mut buf);
                socket.send_to(&buf, (multicast_ip, 60004)).unwrap();
            }
        });
    }

    run()
}
