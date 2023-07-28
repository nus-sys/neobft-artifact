use std::fs::{read_to_string, write};

use neoart::{
    bin::Spec,
    meta::{MULTICAST_CONTROL_RESET_PORT, MULTICAST_PORT},
};

fn main() {
    let spec = toml::from_str::<Spec>(&read_to_string("spec.toml").unwrap()).unwrap();
    write("src-sw/neo_s.py", rewrite_sw(&spec, "neo_s", false)).unwrap();
    write("src-sw/neo_s-sim.py", rewrite_sw(&spec, "neo_s", true)).unwrap();
    write("src-sw/neo_r.py", rewrite_sw(&spec, "neo_r", false)).unwrap();
    write("src-sw/neo_r-sim.py", rewrite_sw(&spec, "neo_r", true)).unwrap();
}

fn rewrite_sw(spec: &Spec, program: &str, simulate: bool) -> String {
    let mut dmac = Vec::new();
    let mut port = Vec::new();
    let mut replicas = Vec::new();
    let mut endpoints = Vec::new();
    if let Some(dev_port) = spec.multicast.accel_port {
        port.push((dev_port, "100G")); //
    }
    for node in &spec.replica {
        let link_speed = if node.link_speed.is_empty() {
            "100G"
        } else {
            &node.link_speed
        };
        dmac.push((&node.link, node.dev_port));
        port.push((node.dev_port, link_speed));
        replicas.push(node.dev_port);
        endpoints.push(node.dev_port);
    }
    for node in &spec.client {
        let link_speed = if node.link_speed.is_empty() {
            "100G"
        } else {
            &node.link_speed
        };
        dmac.push((&node.link, node.dev_port));
        port.push((node.dev_port, link_speed));
        endpoints.push(node.dev_port);
    }
    const GROUP_ENDPOINT: u16 = 1;
    const GROUP_REPLICA: u16 = 2;
    const NODE_ENDPOINT: u16 = 1;
    const NODE_REPLICA: u16 = 2;
    let pre_node = [(NODE_ENDPOINT, endpoints), (NODE_REPLICA, replicas)]
        .into_iter()
        .map(|(group_id, ports)| (group_id, 0xffff, ports))
        .collect::<Vec<_>>();
    let pre_mgid = [
        (GROUP_ENDPOINT, vec![NODE_ENDPOINT]),
        (GROUP_REPLICA, vec![NODE_REPLICA]),
    ];

    let sw = include_str!("spec_sw.in.py");
    sw.replace(r#""@@PROGRAM@@""#, &format!(r#""{program}""#))
        .replace(r#""@@SIMULATE@@""#, if simulate { "True" } else { "False" })
        .replace(r#""@@MULTICAST_PORT@@""#, &MULTICAST_PORT.to_string())
        .replace(
            r#""@@MULTICAST_CONTROL_RESET_PORT@@""#,
            &MULTICAST_CONTROL_RESET_PORT.to_string(),
        )
        .replace(r#""@@DMAC@@""#, &format!("{dmac:?}"))
        .replace(r#""@@PORT@@""#, &format!("{port:?}"))
        .replace(r#""@@GROUP_ENDPOINT@@""#, &GROUP_ENDPOINT.to_string())
        .replace(r#""@@GROUP_REPLICA@@""#, &GROUP_REPLICA.to_string())
        .replace(r#""@@PRE_NODE@@""#, &format!("{pre_node:?}"))
        .replace(r#""@@PRE_MGID@@""#, &format!("{pre_mgid:?}"))
        .replace(
            r#""@@ACCEL_PORT@@""#,
            spec.multicast
                .accel_port
                .as_ref()
                .map(ToString::to_string)
                .as_deref()
                .unwrap_or("..."),
        )
}
