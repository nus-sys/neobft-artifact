use std::{process::Command, thread::spawn};

fn main() {
    let status = Command::new("cargo")
        .args(["build", "--release", "--package", "relay"])
        .status()
        .unwrap();
    assert!(status.success());
    let status = Command::new("cargo")
        .args(["build", "--release", "--bin", "neo-sequencer"])
        .status()
        .unwrap();
    assert!(status.success());

    let output = neo_aws::Output::new_terraform();
    let mut rsync_threads = Vec::from_iter(output.relay_hosts.into_iter().map(|host| {
        spawn(move || {
            let status = Command::new("rsync")
                .arg("target/release/relay")
                .arg(format!("{host}:"))
                .status()
                .unwrap();
            assert!(status.success());
        })
    }));
    rsync_threads.push(spawn(move || {
        let status = Command::new("rsync")
            .arg("target/release/neo-sequencer")
            .arg(format!("{}:", output.sequencer_host))
            .status()
            .unwrap();
        assert!(status.success());
    }));
    for thread in rsync_threads {
        thread.join().unwrap()
    }
}
