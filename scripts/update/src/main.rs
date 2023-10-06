use std::{process::Command, thread::spawn};

const HOSTS: &[&str] = &[
    "nsl-node1.d2",
    //
    "nsl-node10.d2",
];
const WORK_DIR: &str = "/local/cowsay/artifacts";

fn main() {
    let status = Command::new("cargo")
        .args(["build", "--release", "--bin", "replicated"])
        .status()
        .unwrap();
    assert!(status.success());
    let rsync_threads = Vec::from_iter(HOSTS.iter().map(|host| {
        spawn(move || {
            let status = Command::new("rsync")
                .arg("target/release/replicated")
                .arg(format!("{host}:{WORK_DIR}"))
                .status()
                .unwrap();
            assert!(status.success());
        })
    }));
    for thread in rsync_threads {
        thread.join().unwrap()
    }
    //
}
