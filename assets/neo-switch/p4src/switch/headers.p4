header ethernet_h {
#ifdef PKTGEN
	// this is for a quick hack to use pkt gen
	mac_addr_t src_addr;
	mac_addr_t dst_addr;
#else
	mac_addr_t dst_addr;
	mac_addr_t src_addr;
#endif
	ether_type_t ether_type;
}

header reserved_h {
	bit<8> res0;
	bit<8> res1;
	bit<8> res2;
}

header ipv4_h {
	bit<4> version;
	bit<4> ihl;
	bit<8> diffserv;
	bit<16> total_len;
	bit<16> identification;
	bit<3> flags;
	bit<13> frag_offset;
	bit<8> ttl;
	ip_protocol_t protocol;
	bit<16> hdr_checksum;
	ipv4_addr_t src_addr;
	ipv4_addr_t dst_addr;
}

header tcp_h {
	l4_port_t src_port;
	l4_port_t dst_port;
	bit<32> seq_no;
	bit<32> ack_no;
	bit<4> data_offset;
	bit<4> res;
	bit<8> flags;
	bit<16> window;
	bit<16> checksum;
	bit<16> urgent_ptr;
}

header udp_h {
	l4_port_t src_port;
	l4_port_t dst_port;
	bit<16> udp_total_len;
	bit<16> checksum;
}

header bft_h {
	// start: 32 bytes unused
	bit<32> unused0;
	bit<32> unused1;
	bit<32> unused2;
	bit<32> unused3;
	bit<32> unused4;
	// 8 bytes used for digest
	bit<32> digest0;
	bit<32> digest1;
	// 4 bytes used for signalling
	bit<8> 	pad0;
	bit<8> 	pad1;
	bit<8> 	shard_num;
	bit<8> 	reserved;
	// end: 32 bytes

	// 4 bytes sequence number
	bit<32> msg_num;
	// 1 byte session number
	bit<8> 	sess_num;
}

header sip_inout_h {
    bit<32> m_0; 
    bit<32> m_1;
    bit<32> m_2; 
    bit<32> m_3;
}

header sip_out_h {
    bit<32> h_0;
}

header sip_meta_h {
	bit<32> v_0;
	bit<32> v_1;
	bit<32> v_2;
	bit<32> v_3;
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