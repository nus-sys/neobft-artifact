SIMULATE = "@@SIMULATE@@"
PROGRAM = "@@PROGRAM@@"
MULTICAST_PORT = "@@MULTICAST_PORT@@"
MULTICAST_CONTROL_RESET_PORT = "@@MULTICAST_CONTROL_RESET_PORT@@"
MULTICAST_ACCEL_PORT = 60004
DMAC = "@@DMAC@@"
PORT = "@@PORT@@"
PRE_MGID = "@@PRE_MGID@@"
PRE_NODE = "@@PRE_NODE@@"
GROUP_ENDPOINT = "@@GROUP_ENDPOINT@@"
GROUP_REPLICA = "@@GROUP_REPLICA@@"
ACCEL_PORT = "@@ACCEL_PORT@@"

if 0:  # small hack to suppress IDE warning
    bfrt = ...
port = bfrt.port.port
node = bfrt.pre.node
mgid = bfrt.pre.mgid
prog = getattr(bfrt, PROGRAM)
ig = prog.pipe.SwitchIngress
ig_prsr = prog.pipe.SwitchIngressParser

if not SIMULATE:
    port.clear()
    for dev_port, speed in PORT:
        port.add(
            DEV_PORT=dev_port,
            SPEED="BF_SPEED_" + speed,
            FEC="BF_FEC_TYP_NONE",
            PORT_ENABLE=True,
            AUTO_NEGOTIATION="PM_AN_FORCE_DISABLE",
        )

for entry in ig.info(True, False):
    if entry["type"] != "MATCH_DIRECT":
        continue
    entry["node"].clear()
    entry["node"].reset_default()
for entry in node.dump(return_ents=True) or []:
    entry.remove()
for entry in mgid.dump(return_ents=True) or []:
    entry.remove()
ig_prsr.neo_port.clear()
ig_prsr.neo_control_reset_port.clear()
ig_prsr.neo_accel_port.clear()

for dst_addr, port in DMAC:
    ig.dmac.add_with_send(dst_addr=dst_addr, port=port)

for node_id, rid, ports in PRE_NODE:
    node.add(node_id, rid, [], ports)
for group_id, nodes in PRE_MGID:
    mgid.add(group_id, nodes, [0] * len(nodes), [0] * len(nodes))
ig.send_to_endpoints.set_default_with_send_to_group(mgid=GROUP_ENDPOINT)
if PROGRAM == "neo_s":
    ig.send_to_replicas.set_default_with_send_to_group(mgid=GROUP_REPLICA)
if PROGRAM == "neo_r":
    ig.send_to_replicas.set_default_with_send_multicast_to_group(
        dst_port=MULTICAST_PORT, mgid=GROUP_REPLICA
    )
    ig.relay_to_accel.set_default_with_send_multicast(
        dst_port=MULTICAST_ACCEL_PORT, port=ACCEL_PORT
    )
    ig.control_accel.set_default_with_send(port=ACCEL_PORT)

ig_prsr.neo_port.add(MULTICAST_PORT)
ig_prsr.neo_control_reset_port.add(MULTICAST_CONTROL_RESET_PORT)
ig_prsr.neo_accel_port.add(MULTICAST_ACCEL_PORT)
