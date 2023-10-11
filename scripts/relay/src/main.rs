use std::{
    env::args,
    io::ErrorKind,
    iter::repeat,
    net::{Ipv4Addr, UdpSocket},
    sync::Arc,
    thread::{available_parallelism, spawn},
};

use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};

pub fn set_affinity(index: usize) {
    let mut cpu_set = CpuSet::new();
    cpu_set.set(index).unwrap();
    sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap()
}

fn main() {
    let ips = Vec::from_iter(args().skip(1).map(|ip| ip.parse::<Ipv4Addr>().unwrap()));
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:60004").unwrap());
    socket.set_nonblocking(true).unwrap();
    let messages = flume::bounded::<Vec<_>>(1024);
    for ((index, messages), (socket, ips)) in repeat(messages.1)
        .take(usize::from(available_parallelism().unwrap()) - 1)
        .enumerate()
        .zip(repeat((socket.clone(), ips)))
    {
        spawn(move || {
            set_affinity(index + 1);
            loop {
                let buf = messages.recv().unwrap();
                for &ip in &ips {
                    loop {
                        match socket.send_to(&buf, (ip, 60004)) {
                            Ok(_) => break,
                            Err(err) if err.kind() == ErrorKind::WouldBlock => {}
                            err => {
                                err.unwrap();
                            }
                        }
                    }
                }
            }
        });
    }

    set_affinity(0);
    let mut buf = vec![0; 65536];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, _)) => messages.0.send(buf[..len].to_vec()).unwrap(),
            Err(err) if err.kind() == ErrorKind::WouldBlock => {}
            err => {
                err.unwrap();
            }
        }
    }
}
