struct pipe_1_header_t {
    ethernet_h ethernet;
    // reserved_h reserved;
    ipv4_h ipv4;
    udp_h udp;
    bft_h bft;
	
    sip_inout_h sip00;
    sip_inout_h sip01;
    sip_meta_h sip00_meta;
    sip_meta_h sip01_meta;

    sip_inout_h sip10;
    sip_inout_h sip11;
    sip_meta_h sip10_meta;
    sip_meta_h sip11_meta;
}

struct pipe_1_ig_metadata_t {
	bool recirc;
	sip_tmp_h sip10_tmp;
    sip_tmp_h sip11_tmp;
}

struct pipe_1_eg_metadata_t {
	sip_tmp_h sip00_tmp;
    sip_tmp_h sip01_tmp;
}