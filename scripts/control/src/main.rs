use std::{net::SocketAddr, sync::Arc, time::Duration};

use control_messages::{BenchmarkClient, BenchmarkStats, Replica, Role, Task};
use reqwest::Client;
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client_addrs =
        Vec::from_iter((0..100).map(|index| SocketAddr::from(([10, 0, 0, 10], 20000 + index))));
    let replica_addrs = vec![
        SocketAddr::from(([10, 0, 0, 1], 10000)), //
    ];

    let client_host = "nsl-node10.d2";
    let replica_hosts = [
        "nsl-node1.d2", //
    ];

    let client = Arc::new(Client::new());
    let cancel = CancellationToken::new();
    for (index, host) in replica_hosts.into_iter().enumerate() {
        let task = Task {
            client_addrs: client_addrs.clone(),
            replica_addrs: replica_addrs.clone(),
            role: Role::Replica(Replica { index: index as _ }),
        };
        spawn(host_session(host, task, client.clone(), cancel.clone()));
    }

    sleep(Duration::from_secs(1)).await;
    let task = Task {
        client_addrs,
        replica_addrs,
        role: Role::BenchmarkClient(BenchmarkClient {
            num_group: 4,
            num_client: 10,
            duration: Duration::from_secs(10),
        }),
    };
    spawn(host_session(
        client_host,
        task,
        client.clone(),
        cancel.clone(),
    ));

    loop {
        select! {
            _ = sleep(Duration::from_secs(1)) => {}
            _ = cancel.cancelled() => break,
        }
        let response = client
            .get(format!("http://{client_host}:9999/benchmark"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        if let Some(stats) = response.json::<Option<BenchmarkStats>>().await.unwrap() {
            println!("* {stats:?}");
            break;
        }
    }
}

async fn host_session(
    host: impl Into<String>,
    task: Task,
    client: Arc<Client>,
    cancel: CancellationToken,
) {
    let host = host.into();
    let endpoint = format!("http://{host}:9999");
    let response = client
        .post(format!("{endpoint}/task"))
        .json(&task)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    loop {
        select! {
            _ = sleep(Duration::from_secs(1)) => {}
            _ = cancel.cancelled() => break,
        }
        let response = client
            .get(format!("{endpoint}/panic"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        if response.json::<bool>().await.unwrap() {
            println!("! {host} panic");
            cancel.cancel()
        }
    }
}
