use std::{
    process::{Command, Stdio},
    thread::{sleep, spawn},
    time::Duration,
};

const HOSTS: &[&str] = &[
    "nsl-node1.d2",
    //
    "nsl-node10.d2",
];
const WORK_DIR: &str = "/local/cowsay/artifacts";
const PROGRAM: &str = "permissioned-blockchain";

fn main() {
    let status = Command::new("cargo")
        .args(["build", "--release", "--bin", PROGRAM])
        .status()
        .unwrap();
    assert!(status.success());
    let rsync_threads = Vec::from_iter(HOSTS.iter().map(|host| {
        spawn(move || {
            let status = Command::new("rsync")
                .arg(format!("target/release/{PROGRAM}"))
                .arg(format!("{host}:{WORK_DIR}"))
                .status()
                .unwrap();
            assert!(status.success());
            let status = Command::new("ssh")
                .args([host, "pkill", "-INT", "--full", PROGRAM])
                .status()
                .unwrap();
            sleep(Duration::from_secs(1));
            if status.success() {
                let status = Command::new("ssh")
                    .args([host, "pkill", "-KILL", "--full", PROGRAM])
                    .status()
                    .unwrap();
                sleep(Duration::from_secs(1));
                if status.success() {
                    println!("! cleaned nonresponsive server on {host}")
                }
            }
            let status = Command::new("ssh")
                .arg(host)
                .arg(format!("{WORK_DIR}/{PROGRAM} 1>{WORK_DIR}/{PROGRAM}-stdout.txt 2>{WORK_DIR}/{PROGRAM}-stderr.txt &"))
                .status()
                .unwrap();
            assert!(status.success());
            sleep(Duration::from_secs(1));
            let status = Command::new("curl")
                .arg("--silent")
                .arg(format!("http://{host}:9999/panic"))
                .stdout(Stdio::null())
                // .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
            println!("* server started on {host}")
        })
    }));
    for thread in rsync_threads {
        thread.join().unwrap()
    }
}
