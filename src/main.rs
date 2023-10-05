use std::{collections::HashMap, net::SocketAddr, time::Duration};

use replicated::{
    client::Benchmark,
    context::{
        tokio::{Config, Dispatch},
        To,
    },
};

fn main() {
    let mut addrs = HashMap::new();
    for index in 0..100 {
        addrs.insert(
            To::Client(index),
            SocketAddr::from(([10, 0, 0, 10], 20000 + index)),
        );
    }
    addrs.insert(To::Replica(0), SocketAddr::from(([10, 0, 0, 1], 10000)));
    let config = Config::new(addrs);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut dispatch = Dispatch::new(config, runtime.handle().clone());

    if let Some(num_client) = std::env::args().nth(1) {
        let mut bench = Benchmark::new();
        for index in 0..num_client.parse::<u16>().unwrap() {
            let client =
                replicated::unreplicated::Client::new(dispatch.register(To::Client(index)), index);
            bench.insert_client(To::Client(index), client);
        }

        let handle = dispatch.handle();
        let run = bench.run_dispatch();
        let dispatch_thread = std::thread::spawn(move || run(&mut dispatch));

        let bench_thread = std::thread::spawn(move || {
            let secs = 1;
            bench.close_loop(Duration::from_secs(secs));
            handle.stop_sync();
            println!(
                "{} {:?}",
                bench.latencies.len() as f32 / secs as f32,
                bench
                    .latencies
                    .iter()
                    .sum::<Duration>()
                    .checked_div(bench.latencies.len() as u32)
            );
        });

        std::thread::spawn(move || drop(runtime));

        bench_thread.join().unwrap();
        dispatch_thread.join().unwrap();
    } else {
        let mut replica = replicated::unreplicated::Replica::new(dispatch.register(To::Replica(0)));
        let handle = dispatch.handle();
        let dispatch_thread = std::thread::spawn(move || dispatch.run(&mut replica));

        runtime.block_on(async move {
            tokio::signal::ctrl_c().await.unwrap();
            handle.stop().await
        });
        dispatch_thread.join().unwrap();
        runtime.shutdown_background()
    }
}
