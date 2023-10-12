use std::{net::SocketAddr, sync::Arc, time::Duration};

use control_messages::{App, BenchmarkClient, BenchmarkStats, Replica, Role, Task};
use reqwest::Client;
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    run(
        1, //
        1,
        1,
        "unreplicated",
        App::Null,
        0.,
        1,
        &[],
        std::io::empty(),
    )
    .await;
    return;

    let ycsb_app = App::Ycsb(control_messages::YcsbConfig {
        num_key: 10 * 1000,
        num_value: 100 * 1000,
        key_len: 64,
        value_len: 128,
        read_portion: 50,
        update_portion: 40,
        rmw_portion: 10,
    });
    match std::env::args().nth(1).as_deref() {
        Some("fpga") => {
            let saved = std::fs::read_to_string("saved-fpga.csv").unwrap_or_default();
            let saved_lines = Vec::from_iter(saved.lines());
            let mut out = std::fs::File::options()
                .create(true)
                .append(true)
                .open("saved-fpga.csv")
                .unwrap();
            run_clients(
                "unreplicated",
                [1].into_iter().chain((2..=20).step_by(2)),
                &saved_lines,
                &mut out,
            )
            .await;
            run_clients(
                "neo-pk",
                [1].into_iter().chain((2..=40).step_by(2)),
                &saved_lines,
                &mut out,
            )
            .await;
            run_clients(
                "neo-bn",
                [1].into_iter().chain((2..=60).step_by(2)),
                &saved_lines,
                &mut out,
            )
            .await;
            run_clients(
                "pbft",
                [1].into_iter().chain((2..=60).step_by(2)),
                &saved_lines,
                &mut out,
            )
            .await;
            run_clients(
                "zyzzyva",
                [1].into_iter().chain((2..=20).step_by(2)),
                &saved_lines,
                &mut out,
            )
            .await;
            run_clients(
                "zyzzyva-f",
                [1].into_iter().chain((2..=20).step_by(2)),
                &saved_lines,
                &mut out,
            )
            .await;
            run_clients(
                "hotstuff",
                [1].into_iter().chain((2..=60).step_by(2)),
                &saved_lines,
                &mut out,
            )
            .await;
            run_clients(
                "minbft",
                [1].into_iter().chain((2..=60).step_by(2)),
                &saved_lines,
                &mut out,
            )
            .await;

            for mode in [
                "unreplicated",
                "neo-pk",
                "neo-bn",
                "pbft",
                "zyzzyva",
                // "zyzzyva-f",
                "hotstuff",
                "minbft",
            ] {
                run_full_throughput(mode, ycsb_app, 0., 1, &saved_lines, &mut out).await
            }

            for drop_rate in [1e-5, 5e-5, 1e-4, 5e-4, 1e-3] {
                run_full_throughput("neo-pk", App::Null, drop_rate, 1, &saved_lines, &mut out).await
            }
        }
        Some("hmac") => {
            let saved = std::fs::read_to_string("saved-hmac.csv").unwrap_or_default();
            let saved_lines = Vec::from_iter(saved.lines());
            let mut out = std::fs::File::options()
                .create(true)
                .append(true)
                .open("saved-hmac.csv")
                .unwrap();

            run_clients(
                "neo-hm",
                [1].into_iter().chain((2..20).step_by(2)),
                &saved_lines,
                &mut out,
            )
            .await;

            run_full_throughput("neo-hm", ycsb_app, 0., 1, &saved_lines, &mut out).await;

            for drop_rate in [1e-5, 5e-5, 1e-4, 5e-4, 1e-3] {
                run_full_throughput("neo-hm", App::Null, drop_rate, 1, &saved_lines, &mut out).await
            }
        }
        #[cfg(not(feature = "aws"))]
        Some("aws") => panic!("require enable aws feature"),
        #[cfg(feature = "aws")]
        Some("aws") => {
            //
        }

        _ => unimplemented!(),
    }
}

async fn run_full_throughput(
    mode: &str,
    app: App,
    drop_rate: f64,
    num_faulty: usize,
    saved_lines: &[&str],
    out: impl std::io::Write,
) {
    run(
        5,
        100,
        1,
        mode,
        app,
        drop_rate,
        num_faulty,
        saved_lines,
        out,
    )
    .await
}

async fn run_clients(
    mode: &str,
    num_clients_in_5_groups: impl Iterator<Item = usize>,
    saved_lines: &[&str],
    mut out: impl std::io::Write,
) {
    run(1, 1, 1, mode, App::Null, 0., 1, saved_lines, &mut out).await;
    for num_client in num_clients_in_5_groups {
        run(
            5,
            num_client,
            1,
            mode,
            App::Null,
            0.,
            1,
            saved_lines,
            &mut out,
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    num_group: usize,
    num_client: usize,
    num_client_host: usize,
    mode: &str,
    app: App,
    drop_rate: f64,
    num_faulty: usize,
    saved_lines: &[&str],
    mut out: impl std::io::Write,
) {
    let client_addrs;
    let replica_addrs;
    let multicast_addr;
    let client_hosts;
    let replica_hosts;

    #[cfg(not(feature = "aws"))]
    {
        assert!(num_faulty <= 1);
        client_addrs = (20000..).map(|port| SocketAddr::from(([10, 0, 0, 10], port)));
        replica_addrs = vec![
            SocketAddr::from(([10, 0, 0, 1], 10000)),
            SocketAddr::from(([10, 0, 0, 2], 10000)),
            SocketAddr::from(([10, 0, 0, 3], 10000)),
            SocketAddr::from(([10, 0, 0, 4], 10000)),
        ];
        multicast_addr = SocketAddr::from(([10, 0, 0, 255], 60004));

        client_hosts = ["nsl-node10.d2"];
        assert_eq!(num_client_host, 1);
        replica_hosts = [
            "nsl-node1.d2",
            "nsl-node2.d2",
            "nsl-node3.d2",
            "nsl-node4.d2",
        ];
    }

    #[cfg(feature = "aws")]
    {
        use std::net::Ipv4Addr;
        let output = neo_aws::Output::new_terraform();
        client_addrs = output
            .client_ips
            .into_iter()
            .map(|ip| ip.parse::<Ipv4Addr>().unwrap())
            .flat_map(|ip| {
                (20000..)
                    .take(num_group * num_client)
                    .map(move |port| SocketAddr::from((ip, port)))
            });
        replica_addrs = Vec::from_iter(
            output
                .replica_ips
                .into_iter()
                .map(|ip| SocketAddr::from((ip.parse::<Ipv4Addr>().unwrap(), 10000)))
                .chain(
                    (30000..)
                        .map(|port| SocketAddr::from(([127, 0, 0, 1], port)))
                        .take(num_faulty),
                )
                .take(3 * num_faulty + 1),
        );
        multicast_addr =
            SocketAddr::from((output.sequencer_ip.parse::<Ipv4Addr>().unwrap(), 60004));
        client_hosts = output.client_hosts;
        // TODO clarify this and avoid pitfall
        replica_hosts = output.replica_hosts[..2 * num_faulty + 1].to_vec();
    }

    let client_addrs =
        Vec::from_iter(client_addrs.take(num_group * num_client * client_hosts.len()));

    let id = format!(
        "{mode},{},{drop_rate},{},{num_faulty}",
        match app {
            App::Null => "null",
            App::Ycsb(_) => "ycsb",
        },
        client_addrs.len(),
    );
    println!("* work on {id}");
    if saved_lines.iter().any(|line| line.starts_with(&id)) {
        println!("* skip because exist record found");
        return;
    }

    let task = |role| Task {
        mode: String::from(mode),
        app,
        client_addrs: client_addrs.clone(),
        replica_addrs: replica_addrs.clone(),
        multicast_addr,
        num_faulty,
        drop_rate,
        seed: 3603269_3604874,
        role,
    };

    let cancel = CancellationToken::new();
    let hook = std::panic::take_hook();
    std::panic::set_hook({
        let cancel = cancel.clone();
        Box::new(move |info| {
            cancel.cancel();
            hook(info)
        })
    });

    let client = Arc::new(Client::new());
    let mut sessions = Vec::new();
    println!("* start replicas");
    for (index, host) in replica_hosts.into_iter().enumerate() {
        if mode == "unreplicated" && index > 0 {
            break;
        }
        if mode != "zyzzyva" && index >= replica_addrs.len() - num_faulty {
            break;
        }
        #[allow(clippy::int_plus_one)]
        if mode == "minbft" && index >= num_faulty + 1 {
            break;
        }
        sessions.push(spawn(host_session(
            host,
            task(Role::Replica(Replica { index: index as _ })),
            client.clone(),
            cancel.clone(),
        )));
    }

    sleep(Duration::from_secs(1)).await;
    println!("* start clients");
    let mut benchmark = BenchmarkClient {
        num_group,
        num_client,
        offset: 0,
        duration: Duration::from_secs(10),
    };
    for client_host in client_hosts.iter().take(num_client_host) {
        sessions.push(spawn(host_session(
            client_host.to_string(),
            task(Role::BenchmarkClient(benchmark)),
            client.clone(),
            cancel.clone(),
        )));
        benchmark.offset += num_group * num_client;
    }

    for (index, client_host) in client_hosts.into_iter().enumerate().take(num_client_host) {
        loop {
            let response = client
                .get(format!("http://{client_host}:9999/benchmark"))
                .send()
                .await
                .unwrap();
            assert!(response.status().is_success());
            if let Some(stats) = response.json::<Option<BenchmarkStats>>().await.unwrap() {
                println!("* {stats:?}");
                assert_ne!(stats.throughput, 0.);
                writeln!(
                    out,
                    "{id},{index},{},{}",
                    stats.throughput,
                    stats.average_latency.unwrap().as_nanos() as f64 / 1000.,
                )
                .unwrap();
                break;
            }
            select! {
                _ = sleep(Duration::from_secs(1)) => {}
                _ = cancel.cancelled() => break,
            }
        }
    }

    cancel.cancel();
    for session in sessions {
        session.await.unwrap()
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
    let reset = loop {
        select! {
            _ = sleep(Duration::from_secs(1)) => {}
            _ = cancel.cancelled() => break true,
        }
        let response = client
            .get(format!("{endpoint}/panic"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        if response.json::<bool>().await.unwrap() {
            println!("! {host} panic");
            cancel.cancel();
            break false;
        }
    };
    if reset {
        let response = client
            .post(format!("{endpoint}/reset"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
    } else {
        panic!()
    }
}
