struct pipe1_ingress_headers_t {
    ethernet_h   ethernet;
    ipv4_h ipv4;
    udp_h udp;
    bft_h bft;
    sip_inout_h sip;
    sip_meta_h sip_meta;
}

struct pipe1_ingress_metadata_t {
    // bool recirc;
    // bit<9> rnd_port_for_recirc;
    // bit<1> rnd_bit;
    sip_tmp_h sip_tmp;
}

struct pipe1_egress_headers_t {
    ethernet_h   ethernet;
    ipv4_h ipv4;
    udp_h udp;
    bft_h bft;
    sip_inout_h sip;
    sip_meta_h sip_meta;
}

struct pipe1_egress_metadata_t {
    // bool recirc;
    // bit<9> rnd_port_for_recirc;
    // bit<1> rnd_bit;
    sip_tmp_h sip_tmp;
}