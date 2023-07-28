// NeoBFT switch program, relay mode
#include "common.p4"
#include "tofino1arch.p4"
#include "tofino2arch.p4"

control SwitchIngress(
        inout header_t hdr,
        inout metadata_t md,
        in ingress_intrinsic_metadata_t ig_intr_md,
        in ingress_intrinsic_metadata_from_parser_t ig_prsr_md,
        inout ingress_intrinsic_metadata_for_deparser_t ig_dprsr_md,
        inout ingress_intrinsic_metadata_for_tm_t ig_tm_md) {

    action drop() {
        ig_dprsr_md.drop_ctl = 1;
    }

    action send(PortId_t port) {
        ig_tm_md.ucast_egress_port = port;
    }

    action send_to_group(MulticastGroupId_t mgid) {
        ig_tm_md.mcast_grp_a = mgid;
        ig_tm_md.rid = 0xffff;
    }

    table dmac {
        key = { hdr.ethernet.dst_addr : exact; }
        actions = { send; }
        size = 16;
    }

    action send_multicast(bit<16> dst_port, PortId_t port) {
        hdr.udp.checksum = 0;
        hdr.udp.dst_port = dst_port;
        send(port);
    }

    action send_multicast_to_group(bit<16> dst_port, MulticastGroupId_t mgid) {
        hdr.udp.checksum = 0;
        hdr.udp.dst_port = dst_port;
        send_to_group(mgid);
    }

    // keyless tables that always perform default action, need to be configured
    // by control plane

    table relay_to_accel {
        actions = { send_multicast; }
        size = 1;
    }

    table control_accel {
        // reusing received control packet, accelerator accept control packet
        // in the same format
        actions = { send; }
        size = 1;
    }

    table send_to_replicas {
        actions = { send_multicast_to_group; }
        size = 1;
    }

    table send_to_endpoints {
        actions = { send_to_group; }
        size = 1;
    }
   
    apply {
        // No need for egress processing, skip it and use empty controls for egress.
        ig_tm_md.bypass_egress = 1w1;
 
        if (md.code == META_CODE_UNICAST) {
            dmac.apply();
        } else if (md.code == META_CODE_ARP) {
            send_to_endpoints.apply();
        } else if (md.code == META_CODE_MULTICAST) {
            relay_to_accel.apply();
        } else if (md.code == META_CODE_CONTROL_RESET) {
            control_accel.apply();
        } else if (md.code == META_CODE_ACCEL) {
            send_to_replicas.apply();
        } else {
            drop();
        }
    }
}

Pipeline(
        SwitchIngressParser(),
        SwitchIngress(),
        SwitchIngressDeparser(),
        EmptyEgressParser(),
        EmptyEgress(),
        EmptyEgressDeparser()) pipe;

Switch(pipe) main;
