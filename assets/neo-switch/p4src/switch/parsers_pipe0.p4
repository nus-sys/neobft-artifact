parser TomPipe0SwitchIngressParser(
    packet_in pkt,
    out pipe_0_header_t hdr,
    out pipe_0_ig_metadata_t ig_md,
    out ingress_intrinsic_metadata_t ig_intr_md) {

    state start {
        pkt.extract(ig_intr_md);
        pkt.advance(64);    // Tofino 1 port metadata size
        transition parse_ethernet;
    }

    state parse_ethernet {
        pkt.extract(hdr.ethernet);
        transition select(hdr.ethernet.ether_type) {
            ETHERTYPE_IPV4 : parse_ipv4;
            default : reject;
        }
    }

    state parse_ipv4 {
        pkt.extract(hdr.ipv4);
        transition select(hdr.ipv4.protocol) {
            IP_PROTOCOLS_UDP : parse_udp;
            default : accept;
        }
    }

    state parse_udp {
        pkt.extract(hdr.udp);
        transition select(hdr.udp.dst_port) {
            BFT_PORT : parse_bft;
            default : accept;
        }
    }

    // state parse_reserved {
    //     pkt.extract(hdr.reserved);
    //     transition select(hdr.ethernet.dst_addr[47:32], hdr.ethernet.dst_addr[31:24]) {
    //         (0x0100, 0x5e) : parse_bft;
    //         default : accept;
    //     }
    //     // transition parse_bft;
    // }

    state parse_bft {
        pkt.extract(hdr.msg_num);
        pkt.extract(hdr.bft);
        transition accept;
        // transition select(hdr.bft.pad0, hdr.bft.pad1) {
        //     (0, 0)  : parse_digest;
        //     (1, 0)  : parse_s_digest;
        //     // (2, 0)  : parse_s_digest;
        //     // (3, 0)  : parse_s_digest;
        //     default : accept;               
        // }
    }

    // state parse_digest {
    //     pkt.extract(hdr.digest);
    //     transition accept;
    // }

    // state parse_s_digest {
    //     pkt.extract(hdr.s_digest);
    //     transition accept;
    // }
}

control TomPipe0SwitchIngressDeparser(
        packet_out pkt,
        inout pipe_0_header_t hdr,
        in pipe_0_ig_metadata_t ig_md,
        in ingress_intrinsic_metadata_for_deparser_t ig_intr_dprsr_md) {

    apply {
        pkt.emit(hdr);
    }
}


parser TomPipe0SwitchEgressParser(
        packet_in pkt,
        out pipe_0_header_t hdr,
        out pipe_0_eg_metadata_t eg_md,
        out egress_intrinsic_metadata_t eg_intr_md) {
    
    state start {
        pkt.extract(eg_intr_md);
        transition parse_ethernet;
    }

    state parse_ethernet {
        pkt.extract(hdr.ethernet);
        transition select(hdr.ethernet.ether_type) {
            ETHERTYPE_IPV4 : parse_ipv4;
            default : reject;
        }
    }

    // state parse_reserved {
    //     pkt.extract(hdr.reserved);
    //     transition select(hdr.ethernet.dst_addr[47:32], hdr.ethernet.dst_addr[31:24]) {
    //         (0x0100, 0x5e) : parse_bft;
    //         default : accept;
    //     }
    //     // transition parse_bft;
    // }

    state parse_ipv4 {
        pkt.extract(hdr.ipv4);
        transition select(hdr.ipv4.protocol) {
            IP_PROTOCOLS_UDP : parse_udp;
            default : accept;
        }
    }

    state parse_udp {
        pkt.extract(hdr.udp);
        transition select(hdr.udp.dst_port) {
            BFT_PORT : parse_bft;
            default : accept;
        }
    }

    state parse_bft {
        pkt.extract(hdr.msg_num);
        pkt.extract(hdr.bft);
        // pkt.extract(hdr.s_digest);
        transition select(hdr.bft.pad0, hdr.bft.pad1) {
            (0, 128) : parse_sip;
            (1, 128) : parse_sip;
            default : accept;               
        }
    }

    state parse_sip {
        pkt.extract(hdr.sip00);
        pkt.extract(hdr.sip01);
        pkt.extract(hdr.sip10);
        pkt.extract(hdr.sip11);
        // software already allocated
        pkt.extract(hdr.out0);
        pkt.extract(hdr.out1);
        pkt.extract(hdr.out2);
        pkt.extract(hdr.out3);
        transition accept;
    }
}

control TomPipe0SwitchEgressDeparser(
        packet_out pkt,
        inout pipe_0_header_t hdr,
        in pipe_0_eg_metadata_t eg_md,
        in egress_intrinsic_metadata_for_deparser_t eg_intr_md_for_dprsr) {
    
    apply {
        pkt.emit(hdr);
    } 
}