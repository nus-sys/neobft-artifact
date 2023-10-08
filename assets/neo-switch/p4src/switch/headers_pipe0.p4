struct pipe_0_header_t {
    ethernet_h ethernet;
    // reserved_h reserved;
    ipv4_h ipv4;
    udp_h udp;
    bft_h bft;

    sip_inout_h sip00;
    sip_inout_h sip01;
    sip_inout_h sip10;
    sip_inout_h sip11;

    sip_out_h out0;
    sip_out_h out1;
    sip_out_h out2;
    sip_out_h out3;
}

struct pipe_0_ig_metadata_t {

}

struct pipe_0_eg_metadata_t {

}