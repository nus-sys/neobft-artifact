use std::{
    collections::HashMap,
    mem::replace,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router, Server,
};
use control_messages::{BenchmarkStats, Role, Task};
use permissioned_blockchain::{
    app::{ycsb, Workload},
    client::run_benchmark,
    common::set_affinity,
    context::{ordered_multicast::Variant, tokio::Dispatch, Config, Host},
    hotstuff, minbft, neo, pbft, unreplicated, zyzzyva, App,
};
use rand::{rngs::StdRng, SeedableRng};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

enum AppState {
    Idle, // TODO exit on timeout
    Panicked,

    BenchmarkClientRunning,
    BenchmarkClientFinish {
        stats: BenchmarkStats,
    },
    ReplicaRunning {
        cancel: CancellationToken,
        task: JoinHandle<()>,
    },
}

async fn set_task(State(state): State<Arc<Mutex<AppState>>>, Json(task): Json<Task>) {
    assert!(matches!(*state.lock().unwrap(), AppState::Idle));

    let mut addrs = HashMap::new();
    for (index, addr) in task.client_addrs.into_iter().enumerate() {
        addrs.insert(Host::Client(index as _), addr);
    }
    for (index, addr) in task.replica_addrs.into_iter().enumerate() {
        addrs.insert(Host::Replica(index as _), addr);
    }
    let mut dispatch_config = Config::new(addrs, task.num_faulty);
    dispatch_config.multicast_addr = Some(task.multicast_addr);

    let mut rng = StdRng::seed_from_u64(task.seed);
    match task.role {
        Role::BenchmarkClient(config) => {
            *state.lock().unwrap() = AppState::BenchmarkClientRunning;
            let workload = match task.app {
                control_messages::App::Null => Workload::Null,
                control_messages::App::Ycsb(config) => {
                    Workload::Ycsb(ycsb::Workload::new(config.into(), &mut rng))
                }
            };

            let state = state.clone();
            tokio::task::spawn_blocking(move || {
                let latencies = match &*task.mode {
                    "unreplicated" => run_benchmark(
                        dispatch_config,
                        unreplicated::Client::new,
                        config.num_group,
                        config.num_client,
                        config.duration,
                        workload,
                    ),
                    "neo-hm" | "neo-pk" | "neo-bn" => run_benchmark(
                        dispatch_config,
                        neo::Client::new,
                        config.num_group,
                        config.num_client,
                        config.duration,
                        workload,
                    ),
                    "pbft" => run_benchmark(
                        dispatch_config,
                        pbft::Client::new,
                        config.num_group,
                        config.num_client,
                        config.duration,
                        workload,
                    ),
                    "zyzzyva" | "zyzzyva-f" => run_benchmark(
                        dispatch_config,
                        |context, index| {
                            zyzzyva::Client::new(context, index, task.mode == "zyzzyva-f")
                        },
                        config.num_group,
                        config.num_client,
                        config.duration,
                        workload,
                    ),
                    "hotstuff" => run_benchmark(
                        dispatch_config,
                        hotstuff::Client::new,
                        config.num_group,
                        config.num_client,
                        config.duration,
                        workload,
                    ),
                    "minbft" => run_benchmark(
                        dispatch_config,
                        minbft::Client::new,
                        config.num_group,
                        config.num_client,
                        config.duration,
                        workload,
                    ),
                    _ => unimplemented!(),
                };
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
            let app = match task.app {
                control_messages::App::Null => App::Null,
                control_messages::App::Ycsb(config) => {
                    App::Ycsb(ycsb::Workload::app(config.into(), &mut rng))
                }
            };

            let cancel = CancellationToken::new();
            let task = tokio::task::spawn_blocking({
                let cancel = cancel.clone();
                move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    let mut dispatch = Dispatch::new(
                        dispatch_config,
                        runtime.handle().clone(),
                        true,
                        match &*task.mode {
                            "neo-hm" => Variant::new_half_sip_hash(replica.index),
                            "neo-pk" | "neo-bn" => Variant::new_k256(),
                            _ => Variant::Unreachable,
                        },
                    );

                    let handle = dispatch.handle();
                    std::thread::spawn(move || {
                        set_affinity(0);
                        runtime.block_on(async move {
                            cancel.cancelled().await;
                            handle.stop_async().await
                        });
                        runtime.shutdown_background()
                    });

                    set_affinity(1);
                    match &*task.mode {
                        "unreplicated" => {
                            assert_eq!(replica.index, 0);
                            let mut replica = unreplicated::Replica::new(
                                dispatch.register(Host::Replica(0)),
                                app,
                            );
                            // replica.make_blocks = true;
                            dispatch.run(&mut replica)
                        }
                        "neo-hm" | "neo-pk" | "neo-bn" => {
                            let mut replica = neo::Replica::new(
                                dispatch.register(Host::Replica(replica.index)),
                                replica.index,
                                app,
                                task.mode == "neo-bn",
                            );
                            dispatch.drop_rate = task.drop_rate;
                            dispatch.enable_ordered_multicast().run(&mut replica)
                        }
                        "pbft" => {
                            let mut replica = pbft::Replica::new(
                                dispatch.register(Host::Replica(replica.index)),
                                replica.index,
                                app,
                            );
                            dispatch.run(&mut replica)
                        }
                        "zyzzyva" | "zyzzyva-f" => {
                            let mut replica = zyzzyva::Replica::new(
                                dispatch.register(Host::Replica(replica.index)),
                                replica.index,
                                app,
                            );
                            dispatch.run(&mut replica)
                        }
                        "hotstuff" => {
                            let mut replica = hotstuff::Replica::new(
                                dispatch.register(Host::Replica(replica.index)),
                                replica.index,
                                app,
                            );
                            dispatch.run(&mut replica)
                        }
                        "minbft" => {
                            let mut replica = minbft::Replica::new(
                                dispatch.register(Host::Replica(replica.index)),
                                replica.index,
                                app,
                            );
                            dispatch.run(&mut replica)
                        }
                        _ => unimplemented!(),
                    }
                    // TODO return stats
                }
            });
            *state.lock().unwrap() = AppState::ReplicaRunning { cancel, task };
        }
    }
}

async fn poll_benchmark(State(state): State<Arc<Mutex<AppState>>>) -> Json<Option<BenchmarkStats>> {
    let state = state.lock().unwrap();
    match &*state {
        AppState::BenchmarkClientRunning | AppState::Panicked => Json(None),
        &AppState::BenchmarkClientFinish { stats } => Json(Some(stats)),
        _ => unimplemented!(),
    }
}

async fn poll_panic(State(state): State<Arc<Mutex<AppState>>>) -> Json<bool> {
    Json(matches!(*state.lock().unwrap(), AppState::Panicked))
}

async fn reset(State(state): State<Arc<Mutex<AppState>>>) {
    let state = {
        let mut state = state.lock().unwrap();
        replace(&mut *state, AppState::Idle)
    };
    match state {
        AppState::BenchmarkClientFinish { .. } => {}
        AppState::ReplicaRunning { cancel, task } => {
            cancel.cancel();
            task.await.unwrap()
        }
        _ => unimplemented!(),
    }
}

fn main() {
    let state = Arc::new(Mutex::new(AppState::Idle));
    let hook = std::panic::take_hook();
    std::panic::set_hook({
        let state = state.clone();
        Box::new(move |info| {
            *state.lock().unwrap() = AppState::Panicked;
            hook(info)
        })
    });

    let app = Router::new()
        .route("/panic", get(poll_panic))
        .route("/task", post(set_task))
        .route("/reset", post(reset))
        .route("/benchmark", get(poll_benchmark))
        .with_state(state);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(async move {
            Server::bind(&"0.0.0.0:9999".parse().unwrap())
                .serve(app.into_make_service())
                .with_graceful_shutdown(async move { tokio::signal::ctrl_c().await.unwrap() })
                .await
        })
        .unwrap();
    runtime.shutdown_background()
}
