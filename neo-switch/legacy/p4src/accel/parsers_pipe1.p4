parser TomPipe1SwitchIngressParser(packet_in        pkt,
    /* User */
    out pipe1_ingress_headers_t          hdr,
    out pipe1_ingress_metadata_t         meta,
    /* Intrinsic */
    out ingress_intrinsic_metadata_t  ig_intr_md)
{
    /* This is a mandatory state, required by Tofino Architecture */
    state start {
        pkt.extract(ig_intr_md);
        pkt.advance(PORT_METADATA_SIZE);
        transition parse_ethernet;
    }

    state parse_ethernet {
        pkt.extract(hdr.ethernet);
        transition select (hdr.ethernet.ether_type) {
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
            BFT_PORT + 1 : parse_bft_sip_and_meta;
            default : accept;
        }
    }

    state parse_bft_sip_and_meta {
        pkt.extract(hdr.bft);
        pkt.extract(hdr.sip);
        pkt.extract(hdr.sip_meta);
        transition accept;
    }
}

control TomPipe1SwitchIngressDeparser(packet_out pkt,
    /* User */
    inout pipe1_ingress_headers_t                       hdr,
    in    pipe1_ingress_metadata_t                      meta,
    /* Intrinsic */
    in    ingress_intrinsic_metadata_for_deparser_t  ig_dprsr_md)
{
    apply {
        pkt.emit(hdr);
    }
}


parser TomPipe1SwitchEgressParser(packet_in        pkt,
    /* User */
    out pipe1_egress_headers_t          hdr,
    out pipe1_egress_metadata_t         meta,
    /* Intrinsic */
    out egress_intrinsic_metadata_t  eg_intr_md)
{
    /* This is a mandatory state, required by Tofino Architecture */
    state start {
        pkt.extract(eg_intr_md);
        transition parse_ethernet;
    }

    state parse_ethernet {
        pkt.extract(hdr.ethernet);
        transition select (hdr.ethernet.ether_type) {
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
            BFT_PORT : parse_bft_and_sip;
            BFT_PORT + 1 : parse_bft_sip_and_meta;
            default : accept;
        }
    }

    state parse_bft_and_sip {
        pkt.extract(hdr.bft);
        pkt.extract(hdr.sip);
        transition accept;
    }

    state parse_bft_sip_and_meta {
        pkt.extract(hdr.bft);
        pkt.extract(hdr.sip);
        pkt.extract(hdr.sip_meta);
        transition accept;
    }
}

control TomPipe1SwitchEgressDeparser(packet_out pkt,
    /* User */
    inout pipe1_egress_headers_t                       hdr,
    in    pipe1_egress_metadata_t                      meta,
    /* Intrinsic */
    in    egress_intrinsic_metadata_for_deparser_t  eg_dprsr_md)
{
    apply {
        pkt.emit(hdr);
    }
}
