struct pipe0_ingress_headers_t {
    ethernet_h   ethernet;
    ipv4_h ipv4;
    udp_h udp;
    bft_h bft;
    sip_inout_h sip;
}

struct pipe0_ingress_metadata_t {
    bit<10> mirror_session;
}

struct pipe0_egress_headers_t {
    ethernet_h   ethernet;
    ipv4_h ipv4;
    udp_h udp;
    bft_h bft;
}

struct pipe0_egress_metadata_t {
}