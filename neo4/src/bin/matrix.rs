use std::{
    env::args,
    iter::repeat_with,
    net::TcpListener,
    sync::{
        atomic::{AtomicU32, Ordering::SeqCst},
        Arc,
    },
    time::{Duration, Instant},
};

use bincode::Options;
use neoart::{
    bin::{MatrixApp, MatrixArgs, MatrixProtocol},
    crypto::{CryptoMessage, Executor},
    hotstuff,
    meta::{
        Config, OpNumber, ARGS_SERVER_PORT, MULTICAST_ACCEL_PORT, MULTICAST_CONTROL_RESET_PORT,
        MULTICAST_PORT,
    },
    neo, pbft,
    transport::{MulticastListener, Node, Run, Socket, Transport},
    unreplicated, ycsb, zyzzyva, App, Client, minbft,
};
use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};
use rand::{rngs::StdRng, thread_rng, SeedableRng};
use serde::de::DeserializeOwned;
use tokio::{
    net::UdpSocket,
    pin, runtime, select,
    signal::ctrl_c,
    spawn,
    sync::{Mutex, Notify},
    time::sleep,
};

// OVERENGINEERING... bypass command line arguments by setting up a server...
// i learned nothing but these bad practice from tofino SDE :|
fn accept_args() -> MatrixArgs {
    // using std instead of tokio because bincode not natively support async
    let server = TcpListener::bind((
        args().nth(1).as_deref().unwrap_or("0.0.0.0"),
        ARGS_SERVER_PORT,
    ))
    .unwrap();
    let (stream, remote) = server.accept().unwrap();
    println!("* configured by {remote}");
    bincode::options().deserialize_from(&stream).unwrap()
}

const YCSB_N_KEY: usize = 10000;
const YCSB_N_VALUE: usize = 100000;
const YCSB_KEY_LEN: usize = 64;
const YCSB_VALUE_LEN: usize = 128;

fn main() {
    let mut args = accept_args();
    args.config.gen_keys();

    // let pid_file = format!("pid.{}", args.instance_id);
    // using std instead of tokio because i don't want the whole main become
    // async only become of this
    // write(&pid_file, id().to_string()).unwrap();

    let mut executor = Executor::Inline;
    let runtime = match &args.protocol {
        MatrixProtocol::UnreplicatedClient
        | MatrixProtocol::ZyzzyvaClient { .. }
        | MatrixProtocol::NeoClient
        | MatrixProtocol::HotStuffClient
        | MatrixProtocol::PbftClient
        | MatrixProtocol::MinBFTClient => runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(8)
            .on_thread_start({
                let counter = Arc::new(AtomicU32::new(0));
                move || {
                    let mut cpu_set = CpuSet::new();
                    cpu_set.set(counter.fetch_add(1, SeqCst) as _).unwrap();
                    sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
                }
            })
            .build()
            .unwrap(),
        _ => {
            if args.num_worker != 0 {
                executor = Executor::new_rayon(args.num_worker);
            }
            let mut cpu_set = CpuSet::new();
            cpu_set.set(0).unwrap();
            sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
            runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        }
    };
    runtime.block_on(async move {
        let replica_id = args.replica_id;
        let app = args.app;
        let ycsb_app = || {
            ycsb::Workload::new_app(
                YCSB_N_KEY,
                YCSB_KEY_LEN,
                YCSB_VALUE_LEN,
                &mut StdRng::seed_from_u64(0),
            )
        };
        match args.protocol {
            MatrixProtocol::Unknown => unreachable!(),
            MatrixProtocol::UnreplicatedReplica => {
                run_replica(args, executor, |transport| match app {
                    MatrixApp::Null => unreplicated::Replica::new(transport, 0, Null),
                    MatrixApp::Ycsb => unreplicated::Replica::new(transport, 0, ycsb_app()),
                    _ => unreachable!(),
                })
                .await
            }
            MatrixProtocol::UnreplicatedClient => {
                run_clients(args, unreplicated::Client::new).await
            }
            MatrixProtocol::ZyzzyvaReplica { batch_size } => {
                run_replica(args, executor, |transport| match app {
                    MatrixApp::Null => {
                        zyzzyva::Replica::new(transport, replica_id, Null, batch_size)
                    }
                    MatrixApp::Ycsb => {
                        zyzzyva::Replica::new(transport, replica_id, ycsb_app(), batch_size)
                    }
                    _ => unreachable!(),
                })
                .await
            }
            MatrixProtocol::ZyzzyvaClient { assume_byz } => {
                run_clients(args, move |transport| {
                    zyzzyva::Client::new(transport, assume_byz)
                })
                .await
            }
            MatrixProtocol::NeoReplica {
                variant,
                enable_vote,
                batch_size,
            } => {
                let socket = UdpSocket::bind(args.config.multicast).await.unwrap();
                run_replica(args, executor, |mut transport| {
                    transport.listen_multicast(MulticastListener::Os(socket), variant);
                    match app {
                        MatrixApp::Null => {
                            neo::Replica::new(transport, replica_id, Null, enable_vote, batch_size)
                        }
                        MatrixApp::Ycsb => neo::Replica::new(
                            transport,
                            replica_id,
                            ycsb_app(),
                            enable_vote,
                            batch_size,
                        ),
                        _ => unreachable!(),
                    }
                })
                .await
            }
            MatrixProtocol::NeoClient => run_clients(args, neo::Client::new).await,
            MatrixProtocol::PbftReplica { enable_batching } => {
                run_replica(args, executor, |transport| match app {
                    MatrixApp::Null => {
                        pbft::Replica::new(transport, replica_id, Null, enable_batching)
                    }
                    MatrixApp::Ycsb => {
                        pbft::Replica::new(transport, replica_id, ycsb_app(), enable_batching)
                    }
                    _ => unreachable!(),
                })
                .await
            }
            MatrixProtocol::PbftClient => run_clients(args, pbft::Client::new).await,
            MatrixProtocol::HotStuffReplica => {
                run_replica(args, executor, |transport| match app {
                    MatrixApp::Null => hotstuff::Replica::new(transport, replica_id, Null),
                    MatrixApp::Ycsb => hotstuff::Replica::new(transport, replica_id, ycsb_app()),
                    _ => unreachable!(),
                })
                .await
            }
            MatrixProtocol::HotStuffClient => run_clients(args, hotstuff::Client::new).await,
            MatrixProtocol::MinBFTReplica => {
                run_replica(args, executor, |transport| match app {
                    MatrixApp::Null => minbft::Replica::new(transport, replica_id, Null),
                    MatrixApp::Ycsb => minbft::Replica::new(transport, replica_id, ycsb_app()),
                    _ => unreachable!(),
                })
                .await
            }
            MatrixProtocol::MinBFTClient => run_clients(args, minbft::Client::new).await,
        }
    });
}

struct Null;
impl App for Null {
    fn replica_upcall(&mut self, _: OpNumber, _: &[u8]) -> Vec<u8> {
        Vec::default()
    }
}

async fn run_replica<T>(
    args: MatrixArgs,
    executor: Executor,
    new_replica: impl FnOnce(Transport<T>) -> T,
) where
    T: Node + AsMut<Transport<T>> + Send,
    T::Message: CryptoMessage + DeserializeOwned,
{
    let socket = UdpSocket::bind(args.config.replicas[args.replica_id as usize])
        .await
        .unwrap();
    socket.set_broadcast(true).unwrap();
    socket.writable().await.unwrap();
    let mut transport = Transport::new(args.config, Socket::Os(socket), executor);
    transport.drop_rate = args.drop_rate;
    new_replica(transport)
        .run(async { ctrl_c().await.unwrap() })
        .await;
}

async fn run_clients<T>(
    args: MatrixArgs,
    new_client: impl FnOnce(Transport<T>) -> T + Clone + Send + 'static,
) where
    T: Node + Client + Run + Send + 'static,
    T::Message: CryptoMessage + DeserializeOwned,
{
    if args.num_client == 0 {
        return;
    }
    let notify = Arc::new(Notify::new());
    let latencies = Arc::new(Mutex::new(Vec::new()));
    let throughput = Arc::new(AtomicU32::new(0));
    let clients = match args.app {
        MatrixApp::Null => repeat_with(|| {
            let config = args.config.clone();
            let notify = notify.clone();
            let latencies = latencies.clone();
            let throughput = throughput.clone();
            let new_client = new_client.clone();
            let host = args.host.clone();
            spawn(run_null_client(
                config, notify, latencies, throughput, new_client, host,
            ))
        })
        .take(args.num_client as _)
        .collect::<Vec<_>>(),
        MatrixApp::Ycsb => {
            let workload = Arc::new(ycsb::Workload::new(
                YCSB_N_KEY,
                YCSB_N_VALUE,
                YCSB_KEY_LEN,
                YCSB_VALUE_LEN,
                50,
                50,
                0,
                &mut StdRng::seed_from_u64(0),
            ));
            repeat_with(|| {
                let config = args.config.clone();
                let notify = notify.clone();
                let latencies = latencies.clone();
                let throughput = throughput.clone();
                let new_client = new_client.clone();
                let host = args.host.clone();
                let workload = workload.clone();
                let rand = StdRng::from_rng(thread_rng()).unwrap();
                spawn(run_ycsb_client(
                    config, workload, rand, notify, latencies, throughput, new_client, host,
                ))
            })
            .take(args.num_client as _)
            .collect()
        }
        _ => unreachable!(),
    };

    // let mut accumulated_latencies = Vec::new();
    for _ in 0..20 {
        sleep(Duration::from_secs(1)).await;
        // let latencies = take(&mut *latencies.lock().await);
        // println!("* interval throughput {} ops/sec", latencies.len());
        println!(
            "* interval throughput {} ops/sec",
            throughput.swap(0, SeqCst)
        );
        // accumulated_latencies.extend(latencies);
    }
    notify.notify_waiters();
    for client in clients {
        client.await.unwrap();
    }

    let accumulated_latencies = &mut *latencies.lock().await;
    accumulated_latencies.sort_unstable();
    if !accumulated_latencies.is_empty() {
        println!(
            "* 50th {:?} 99th {:?}",
            accumulated_latencies[accumulated_latencies.len() / 2],
            accumulated_latencies[accumulated_latencies.len() / 100 * 99]
        );
    }
}

async fn run_null_client<T>(
    config: Config,
    notify: Arc<Notify>,
    latencies: Arc<Mutex<Vec<Duration>>>,
    throughput: Arc<AtomicU32>,
    new_client: impl FnOnce(Transport<T>) -> T + Send + 'static,
    host: String,
) where
    T: Node + Client + Run + Send,
    T::Message: CryptoMessage + DeserializeOwned,
{
    let socket = UdpSocket::bind((host, 0)).await.unwrap();
    if [
        MULTICAST_PORT,
        MULTICAST_CONTROL_RESET_PORT,
        MULTICAST_ACCEL_PORT,
    ]
    .contains(&socket.local_addr().unwrap().port())
    {
        println!("* client {socket:?} keeps silence");
        return;
    }
    socket.set_broadcast(true).unwrap();
    socket.writable().await.unwrap();
    let transport = Transport::new(config, Socket::Os(socket), Executor::Inline);
    let mut client = new_client(transport);

    let notified = notify.notified();
    pin!(notified);

    let mut closed = false;
    let mut local_latencies = Vec::new();
    while !closed {
        let instant = Instant::now();
        let result = client.invoke(&[]);
        client
            .run(async {
                select! {
                    _ = result => {
                        // latencies.lock().await.push(Instant::now() - instant);
                        local_latencies.push(Instant::now() - instant);
                        throughput.fetch_add(1, SeqCst);
                    }
                    _ = &mut notified => closed = true,
                }
            })
            .await;
    }
    latencies.lock().await.extend(local_latencies);
}

async fn run_ycsb_client<T>(
    config: Config,
    workload: Arc<ycsb::Workload>,
    mut rand: StdRng,
    notify: Arc<Notify>,
    latencies: Arc<Mutex<Vec<Duration>>>,
    throughput: Arc<AtomicU32>,
    new_client: impl FnOnce(Transport<T>) -> T + Send + 'static,
    host: String,
) where
    T: Client + Run + Node,
{
    // TODO duplicated
    let socket = UdpSocket::bind((host, 0)).await.unwrap();
    if [MULTICAST_PORT, MULTICAST_CONTROL_RESET_PORT].contains(&socket.local_addr().unwrap().port())
    {
        println!("* client {socket:?} keeps silence");
        return;
    }
    socket.set_broadcast(true).unwrap();
    socket.writable().await.unwrap();
    let transport = Transport::new(config, Socket::Os(socket), Executor::Inline);
    let mut client = new_client(transport);

    let notified = notify.notified();
    pin!(notified);

    let mut local_latencies = Vec::new();
    loop {
        let instant = Instant::now();
        select! {
            _ = workload.invoke(&mut client, &mut rand) => {
                // latencies.lock().await.push(Instant::now() - instant);
                local_latencies.push(Instant::now() - instant);
                throughput.fetch_add(1, SeqCst);
            }
            _ = &mut notified => break,
        }
    }
    latencies.lock().await.extend(local_latencies);
}
