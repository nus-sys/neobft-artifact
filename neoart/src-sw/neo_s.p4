// NeoBFT switch program, signing mode
#include "common.p4"
// workaround for `p4_build.sh` script. the header seems to be renamed as
// `tofino1_arch.p4` now
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

    // keyless tables that always perform default action, need to be configured
    // by control plane

    table send_to_replicas {
        actions = { send_to_group; }
        size = 1;
    }

    table send_to_endpoints {
        actions = { send_to_group; }
        size = 1;
    }

    bit<32> sequence_number;

    Register<bit<32>, _>(1, 0) sequence;
    RegisterAction<bit<32>, _, bit<32>>(sequence) assign_sequence = {
        void apply(inout bit<32> reg, out bit<32> result) {
            if (md.code == META_CODE_CONTROL_RESET) {
                reg = 0;
                result = 0;
            } else {
                reg = reg + 1;
                result = reg;
            }
        }
    };

    action neo() {
        hdr.udp.checksum = 0;
        hdr.neo.sequence = sequence_number;
        // TODO signature
        bit<8> n1 = (bit<8>) hdr.neo.sequence;
        bit<8> n2 = (bit<8>) hdr.neo.sequence + 1;
        hdr.neo.signature[7:0] = n1;
        hdr.neo.signature[15:8] = n2;
        hdr.neo.hash = 0;
    }

    apply {
        // No need for egress processing, skip it and use empty controls for egress.
        ig_tm_md.bypass_egress = 1w1;

        if (md.code == META_CODE_UNICAST) { 
            dmac.apply(); 
        } else if (md.code == META_CODE_ARP) { 
            send_to_endpoints.apply(); // careful...
        } else if (md.code == META_CODE_MULTICAST) {
            sequence_number = assign_sequence.execute(0);
            neo();
            send_to_replicas.apply();
        } else if (md.code == META_CODE_CONTROL_RESET) {
            assign_sequence.execute(0);
            drop();
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
