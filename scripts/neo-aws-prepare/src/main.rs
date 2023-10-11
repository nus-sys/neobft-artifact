use std::{process::Command, thread::spawn};

fn main() {
    let status = Command::new("cargo")
        .args(["build", "--release", "--package", "relay"])
        .status()
        .unwrap();
    assert!(status.success());
    let status = Command::new("cargo")
        .args(["build", "--release", "--package", "neo-sequencer"])
        .status()
        .unwrap();
    assert!(status.success());

    #[derive(serde::Deserialize)]
    struct Output {
        #[serde(rename = "sequencer-host")]
        sequencer_host: StringValue,
        #[serde(rename = "relays-host")]
        relay_hosts: StringValues,
    }
    #[derive(serde::Deserialize)]
    struct StringValue {
        value: String,
    }
    #[derive(serde::Deserialize)]
    struct StringValues {
        value: Vec<String>,
    }
    let output = Command::new("terraform")
        .args(["-chdir=scripts/aws", "output", "-json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = serde_json::from_slice::<Output>(&output.stdout).unwrap();
    let mut rsync_threads = Vec::from_iter(output.relay_hosts.value.iter().map(|host| {
        let host = host.to_string();
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
            .arg(format!("{}:", output.sequencer_host.value))
            .status()
            .unwrap();
        assert!(status.success());
    }));
    for thread in rsync_threads {
        thread.join().unwrap()
    }
}
