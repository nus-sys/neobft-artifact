header ethernet_h {
    bit<48>   dst_addr;
    bit<48>   src_addr;
    bit<16>   ether_type;
}

header ipv4_h {
    bit<4>   version;
    bit<4>   ihl;
    bit<8>   diffserv;
    bit<16>  total_len;
    bit<16>  identification;
    bit<3>   flags;
    bit<13>  frag_offset;
    bit<8>   ttl;
    bit<8>   protocol;
    bit<16>  hdr_checksum;
    bit<32>  src_addr;
    bit<32>  dst_addr;
}

header udp_h {
	bit<16> src_port;
	bit<16> dst_port;  
	bit<16> udp_total_len;
	bit<16> checksum;   
}

header bft_h {
    bit<32> padding;

    /* 128 bit input for HalfSipHash */ 
    bit<16> sess_num;
    bit<16> shard_num;      // additional digest
    bit<32> msg_num;
    bit<32> prev_hash;      // previous packet's hash
    bit<32> digest;         // computed digest
}

header sip_inout_h {
    bit<32> m_0; 
    bit<32> m_1;
    bit<32> m_2; 
    bit<32> m_3;
}

header sip_meta_h {
	bit<32> v_0;
	bit<32> v_1;
	bit<32> v_2;
	bit<32> v_3;
    bit<16> dest_port;
	bit<8> curr_round;
}

header sip_tmp_h {
	bit<32> a_0;
	bit<32> a_1;
	bit<32> a_2;
	bit<32> a_3;
	bit<32> i_0;
	bit<32> i_1;
	bit<32> i_2;
	bit<32> i_3;
	bit<8> round_type;
}
