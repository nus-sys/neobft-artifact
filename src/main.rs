use std::{
    collections::HashMap,
    iter::repeat,
    net::SocketAddr,
    sync::{Arc, Barrier},
    thread::JoinHandle,
    time::Duration,
};

use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};
use replicated::{
    client::Benchmark,
    context::{
        tokio::{Config, Dispatch, DispatchHandle},
        To,
    },
};
use tokio_util::sync::CancellationToken;

fn set_affinity(index: usize) {
    let mut cpu_set = CpuSet::new();
    cpu_set.set(index).unwrap();
    sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap()
}

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

    if let Some(num_client) = std::env::args().nth(1) {
        let num_client = num_client.parse::<u16>().unwrap();
        let config = Arc::new(config);
        let secs = 10;

        struct Group {
            benchmark_thread: JoinHandle<Benchmark<replicated::unreplicated::Client>>,
            runtime_thread: JoinHandle<()>,
            dispatch_thread: JoinHandle<()>,
            dispatch_handle: DispatchHandle,
        }

        let num_group = std::env::args()
            .nth(2)
            .map(|s| s.parse::<usize>().unwrap())
            .unwrap_or(1);
        let barrier = Arc::new(Barrier::new(num_group));
        let groups = Vec::from_iter(repeat(barrier).take(num_group).enumerate().map(
            |(group_index, barrier)| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let mut dispatch = Dispatch::new(config.clone(), runtime.handle().clone());

                let mut benchmark = Benchmark::new();
                for group_offset in 0..num_client {
                    let index = group_index as u16 * num_client + group_offset;
                    let client = replicated::unreplicated::Client::new(
                        dispatch.register(To::Client(index)),
                        index,
                    );
                    benchmark.insert_client(To::Client(index), client);
                }

                let cancel = CancellationToken::new();
                let runtime_thread = std::thread::spawn({
                    set_affinity(group_index * 3);
                    let cancel = cancel.clone();
                    move || runtime.block_on(cancel.cancelled())
                });

                let dispatch_handle = dispatch.handle();
                let run = benchmark.run_dispatch();
                let dispatch_thread = std::thread::spawn(move || {
                    set_affinity(group_index * 3 + 1);
                    run(&mut dispatch);
                    cancel.cancel()
                });

                let benchmark_thread = std::thread::spawn(move || {
                    set_affinity(group_index * 3 + 2);
                    barrier.wait();
                    benchmark.close_loop(Duration::from_secs(secs), true);
                    benchmark
                });

                Group {
                    benchmark_thread,
                    runtime_thread,
                    dispatch_thread,
                    dispatch_handle,
                }
            },
        ));

        let mut latencies = Vec::new();
        for group in groups {
            let benchmark = group.benchmark_thread.join().unwrap();
            latencies.extend(benchmark.latencies);
            group.dispatch_handle.stop_sync();
            group.dispatch_thread.join().unwrap();
            group.runtime_thread.join().unwrap();
        }
        println!(
            "{} {:?}",
            latencies.len() as f32 / secs as f32,
            latencies
                .iter()
                .sum::<Duration>()
                .checked_div(latencies.len() as u32)
        );
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let dispatch = Dispatch::new(config, runtime.handle().clone());

        let handle = dispatch.handle();
        std::thread::spawn(move || {
            set_affinity(0);
            runtime.block_on(async move {
                tokio::signal::ctrl_c().await.unwrap();
                handle.stop().await
            });
            runtime.shutdown_background()
        });

        set_affinity(1);
        let mut replica = replicated::unreplicated::Replica::new(dispatch.register(To::Replica(0)));
        dispatch.run(&mut replica)
    }
}
