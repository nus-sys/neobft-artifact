use std::{net::SocketAddr, sync::Arc, time::Duration};

use control_messages::{App, BenchmarkClient, BenchmarkStats, Replica, Role, Task};
use reqwest::Client;
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    run(
        BenchmarkClient {
            num_group: 5,
            num_client: 20,
            duration: Duration::from_secs(10),
        },
        "neo-pk",
        App::Null,
        0.,
        4,
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
    let full_throughput = BenchmarkClient {
        num_group: 5,
        num_client: 100,
        duration: Duration::from_secs(10),
    };
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
                run(
                    full_throughput,
                    mode,
                    ycsb_app,
                    0.,
                    4,
                    &saved_lines,
                    &mut out,
                )
                .await
            }

            for drop_rate in [1e-5, 5e-5, 1e-4, 5e-4, 1e-3] {
                run(
                    full_throughput,
                    "neo-pk",
                    App::Null,
                    drop_rate,
                    4,
                    &saved_lines,
                    &mut out,
                )
                .await
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

            run(
                full_throughput,
                "neo-hm",
                ycsb_app,
                0.,
                4,
                &saved_lines,
                &mut out,
            )
            .await;

            for drop_rate in [1e-5, 5e-5, 1e-4, 5e-4, 1e-3] {
                run(
                    full_throughput,
                    "neo-hm",
                    App::Null,
                    drop_rate,
                    4,
                    &saved_lines,
                    &mut out,
                )
                .await
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

async fn run_clients(
    mode: &str,
    num_clients_in_5_groups: impl Iterator<Item = usize>,
    saved_lines: &[&str],
    mut out: impl std::io::Write,
) {
    let mut benchmark = BenchmarkClient {
        num_group: 1,
        num_client: 1,
        duration: Duration::from_secs(10),
    };
    run(benchmark, mode, App::Null, 0., 4, saved_lines, &mut out).await;
    benchmark.num_group = 5;
    for num_client in num_clients_in_5_groups {
        benchmark.num_client = num_client;
        run(benchmark, mode, App::Null, 0., 4, saved_lines, &mut out).await
    }
}

async fn run(
    benchmark: BenchmarkClient,
    mode: &str,
    app: App,
    drop_rate: f64,
    num_replica: usize,
    saved_lines: &[&str],
    mut out: impl std::io::Write,
) {
    let id = format!(
        "{mode},{},{drop_rate},{}",
        match app {
            App::Null => "null",
            App::Ycsb(_) => "ycsb",
        },
        benchmark.num_group * benchmark.num_client,
    );
    println!("* work on {id}");
    if saved_lines.iter().any(|line| line.starts_with(&id)) {
        println!("* skip because exist record found");
        return;
    }

    let num_faulty = (num_replica - 1) / 3;

    let client_addrs;
    let replica_addrs;
    let multicast_addr;
    let client_host;
    let replica_hosts;

    #[cfg(not(feature = "aws"))]
    {
        client_addrs = (20000..).map(|port| SocketAddr::from(([10, 0, 0, 10], port)));
        replica_addrs = vec![
            SocketAddr::from(([10, 0, 0, 1], 10000)),
            SocketAddr::from(([10, 0, 0, 2], 10000)),
            SocketAddr::from(([10, 0, 0, 3], 10000)),
            SocketAddr::from(([10, 0, 0, 4], 10000)),
        ];
        multicast_addr = SocketAddr::from(([10, 0, 0, 255], 60004));

        client_host = "nsl-node10.d2";
        replica_hosts = [
            "nsl-node1.d2",
            "nsl-node2.d2",
            "nsl-node3.d2",
            "nsl-node4.d2",
        ];
    }

    #[cfg(feature = "aws")]
    {
        use std::{iter::repeat, net::Ipv4Addr};
        let output = neo_aws::Output::new_terraform();
        let client_ip = output.client_ip.parse::<Ipv4Addr>().unwrap();
        client_addrs = (20000..).map(move |port| SocketAddr::from((client_ip, port)));
        replica_addrs = Vec::from_iter(
            output
                .replica_ips
                .into_iter()
                .take(2 * num_faulty + 1)
                .map(|ip| SocketAddr::from((ip.parse::<Ipv4Addr>().unwrap(), 10000)))
                .chain(repeat(SocketAddr::from(([127, 0, 0, 1], 19999))).take(num_faulty)),
        );
        multicast_addr =
            SocketAddr::from((output.sequencer_ip.parse::<Ipv4Addr>().unwrap(), 60004));
        client_host = output.client_host;
        // TODO clarify this and avoid pitfall
        replica_hosts = output.replica_hosts[..2 * num_faulty + 1].to_vec();
    }

    let num_client_host = 1;
    let client_addrs = Vec::from_iter(
        client_addrs.take(benchmark.num_group * benchmark.num_client * num_client_host),
    );

    let task = |role| Task {
        mode: String::from(mode),
        app,
        client_addrs: client_addrs.clone(),
        replica_addrs: replica_addrs.clone(),
        multicast_addr,
        num_faulty,
        drop_rate,
        // drop_rate: 1e-3,
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
    sessions.push(spawn(host_session(
        client_host.to_string(),
        task(Role::BenchmarkClient(benchmark)),
        client.clone(),
        cancel.clone(),
    )));

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
            assert_ne!(stats.throughput, 0.);
            writeln!(
                out,
                "{id},{},{}",
                stats.throughput,
                stats.average_latency.unwrap().as_nanos() as f64 / 1000.,
            )
            .unwrap();
            break;
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
