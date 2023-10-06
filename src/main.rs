use std::{
    collections::HashMap,
    iter::repeat,
    sync::{Arc, Barrier, Mutex},
    thread::JoinHandle,
    time::Duration,
};

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router, Server,
};
use control_messages::{BenchmarkStats, Role, Task};
use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};
use replicated::{
    client::Benchmark,
    context::{
        tokio::{Config, Dispatch, DispatchHandle},
        ClientIndex, To,
    },
};
use tokio_util::sync::CancellationToken;

fn set_affinity(index: usize) {
    let mut cpu_set = CpuSet::new();
    cpu_set.set(index).unwrap();
    sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap()
}

enum AppState {
    Idle,
    BenchmarkClientRunning,
    BenchmarkClientFinish { stats: BenchmarkStats },
    ReplicaRunning,
}

async fn set_task(State(state): State<Arc<Mutex<AppState>>>, Json(task): Json<Task>) {
    assert!(matches!(*state.lock().unwrap(), AppState::Idle));

    let mut addrs = HashMap::new();
    for (index, addr) in task.client_addrs.into_iter().enumerate() {
        addrs.insert(To::Client(index as _), addr);
    }
    for (index, addr) in task.replica_addrs.into_iter().enumerate() {
        addrs.insert(To::Replica(index as _), addr);
    }
    let dispatch_config = Config::new(addrs);

    match task.role {
        Role::BenchmarkClient(config) => {
            *state.lock().unwrap() = AppState::BenchmarkClientRunning;
            let state = state.clone();
            tokio::task::spawn_blocking(move || {
                struct Group {
                    benchmark_thread: JoinHandle<Benchmark<replicated::unreplicated::Client>>,
                    runtime_thread: JoinHandle<()>,
                    dispatch_thread: JoinHandle<()>,
                    dispatch_handle: DispatchHandle,
                }

                let barrier = Arc::new(Barrier::new(config.num_group));
                let groups =
                    Vec::from_iter(repeat(barrier).take(config.num_group).enumerate().map(
                        |(group_index, barrier)| {
                            let runtime = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .unwrap();
                            let mut dispatch =
                                Dispatch::new(dispatch_config.clone(), runtime.handle().clone());

                            let mut benchmark = Benchmark::new();
                            for group_offset in 0..config.num_client {
                                let index =
                                    (group_index * config.num_client + group_offset) as ClientIndex;
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
                                benchmark.close_loop(config.duration, true);
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

                *state.lock().unwrap() = AppState::BenchmarkClientFinish {
                    stats: BenchmarkStats {
                        throughput: latencies.len() as f32 / config.duration.as_secs_f32(),
                        average_latency: latencies
                            .iter()
                            .sum::<Duration>()
                            .checked_div(latencies.len() as u32),
                    },
                };
            });
        }
        Role::Replica(replica) => {
            *state.lock().unwrap() = AppState::ReplicaRunning;
            tokio::task::spawn_blocking(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let dispatch = Dispatch::new(dispatch_config, runtime.handle().clone());

                // let handle = dispatch.handle();
                std::thread::spawn(move || {
                    set_affinity(0);
                    // runtime.block_on(async move {
                    //     tokio::signal::ctrl_c().await.unwrap();
                    //     handle.stop().await
                    // });
                    runtime.block_on(std::future::pending::<()>());
                    runtime.shutdown_background()
                });

                set_affinity(1);
                assert_eq!(replica.index, 0);
                let mut replica =
                    replicated::unreplicated::Replica::new(dispatch.register(To::Replica(0)));
                dispatch.run(&mut replica)
            });
        }
    }
}

async fn poll_benchmark(State(state): State<Arc<Mutex<AppState>>>) -> Json<Option<BenchmarkStats>> {
    let state = state.lock().unwrap();
    match &*state {
        AppState::BenchmarkClientRunning => Json(None),
        AppState::BenchmarkClientFinish { stats } => Json(Some(stats.clone())),
        _ => unimplemented!(),
    }
}

fn main() {
    // let mut addrs = HashMap::new();
    // for index in 0..100 {
    //     addrs.insert(
    //         To::Client(index),
    //         SocketAddr::from(([10, 0, 0, 10], 20000 + index)),
    //     );
    // }
    // addrs.insert(To::Replica(0), SocketAddr::from(([10, 0, 0, 1], 10000)));
    // let config = Config::new(addrs);

    let app = Router::new()
        .route("/task", post(set_task))
        .route("/benchmark", get(poll_benchmark))
        .with_state(Arc::new(Mutex::new(AppState::Idle)));
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            Server::bind(&"0.0.0.0:9999".parse().unwrap())
                .serve(app.into_make_service())
                .with_graceful_shutdown(async move { tokio::signal::ctrl_c().await.unwrap() })
                .await
        })
        .unwrap()
}
