use std::{
    collections::HashMap,
    mem::replace,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    client::run_benchmark,
    common::set_affinity,
    context::{
        tokio::{Config, Dispatch},
        Host,
    },
};

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router, Server,
};
use control_messages::{BenchmarkStats, Role, Task};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub mod app;
pub mod client;
pub mod common;
pub mod context;
pub mod unreplicated;

pub use app::App;
pub use client::Client;
pub use context::Context;

enum AppState {
    Idle,
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
    let dispatch_config = Config::new(addrs);

    match task.role {
        Role::BenchmarkClient(config) => {
            *state.lock().unwrap() = AppState::BenchmarkClientRunning;
            let state = state.clone();
            tokio::task::spawn_blocking(move || {
                let latencies = run_benchmark(
                    dispatch_config,
                    config.num_group,
                    config.num_client,
                    config.duration,
                );
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
            let cancel = CancellationToken::new();
            let task = tokio::task::spawn_blocking({
                let cancel = cancel.clone();
                move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    let dispatch = Dispatch::new(dispatch_config, runtime.handle().clone(), true);

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
                    assert_eq!(replica.index, 0);
                    let mut replica =
                        unreplicated::Replica::new(dispatch.register(Host::Replica(0)), App::Null);
                    dispatch.run(&mut replica)
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
        AppState::BenchmarkClientFinish { stats } => Json(Some(stats.clone())),
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
    if std::env::args().nth(1).as_deref() == Some("start-daemon") {
        let current_exe = std::env::current_exe().unwrap();
        let start_script = current_exe.with_file_name("replicated-start.sh");
        std::fs::write(
            &start_script,
            format!(
                "RUST_BACKTRACE=1 {} 1>{} 2>{} &",
                current_exe.display(),
                current_exe
                    .with_file_name("replicated-stdout.txt")
                    .display(),
                current_exe
                    .with_file_name("replicated-stderr.txt")
                    .display()
            ),
        )
        .unwrap();
        let status = std::process::Command::new("bash")
            .arg(start_script)
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

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
