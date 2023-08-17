parser TomPipe0SwitchIngressParser(packet_in        pkt,
    /* User */
    out pipe0_ingress_headers_t          hdr,
    out pipe0_ingress_metadata_t         meta,
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
            BFT_PORT : parse_bft;
            BFT_HOLD : parse_bft;
            BFT_PORT + 1 : parse_bft_and_sip;
            default : accept;
        }
    }

    state parse_bft {
        pkt.extract(hdr.bft);
        transition accept;
    }

    state parse_bft_and_sip {
        pkt.extract(hdr.bft);
        pkt.extract(hdr.sip);
        transition accept;
    }
}

control TomPipe0SwitchIngressDeparser(packet_out pkt,
    /* User */
    inout pipe0_ingress_headers_t                       hdr,
    in    pipe0_ingress_metadata_t                      meta,
    /* Intrinsic */
    in    ingress_intrinsic_metadata_for_deparser_t  ig_dprsr_md)
{
    // Mirror() mirror;
    
    apply {
        // if(ig_dprsr_md.mirror_type == MIRROR_TYPE_ACCEL) {
        //     mirror.emit(meta.mirror_session);
        // }

        pkt.emit(hdr);
    }
}


parser TomPipe0SwitchEgressParser(packet_in        pkt,
    /* User */
    out pipe0_egress_headers_t          hdr,
    out pipe0_egress_metadata_t         meta,
    /* Intrinsic */
    out egress_intrinsic_metadata_t  eg_intr_md)
{
    /* This is a mandatory state, required by Tofino Architecture */
    state start {
        pkt.extract(eg_intr_md);
        transition accept;
    }
}

control TomPipe0SwitchEgressDeparser(packet_out pkt,
    /* User */
    inout pipe0_egress_headers_t                       hdr,
    in    pipe0_egress_metadata_t                      meta,
    /* Intrinsic */
    in    egress_intrinsic_metadata_for_deparser_t  eg_dprsr_md)
{
    apply {
        pkt.emit(hdr);
    }
}
