// action route_to(bit<9> port){
//   // ig_tm_md.ucast_egress_port=port;
// }

// action routing_decision(){
//   //packet routing: for now we simply bounce back the packet.
//   //any routing match-action logic should be added here.
//   // hdr.sip_meta.dest_port=(bit<16>) ig_intr_md.ingress_port;
// }

//  action do_recirculate(){
//   route_to(meta.rnd_port_for_recirc);
//  }

 action incr_and_recirc(bit<8> next_round){
  hdr.sip_meta.curr_round = next_round;
  // do_recirculate();
  //hdr.sip_meta.setValid();
  hdr.udp.dst_port=BFT_PORT +1;
 }

 action do_not_recirc_end_in_ig(){
  // route_to((bit<9>)hdr.sip_meta.dest_port);
  // hdr.udp.dst_port=5555;

  hdr.sip.m_0 = 0; hdr.sip.m_1 = 0; hdr.sip.m_2 = 0; hdr.sip.m_3 = 0;
  @in_hash { hdr.sip.m_0=hdr.sip_meta.v_0 ^ hdr.sip_meta.v_1 ^ hdr.sip_meta.v_2 ^ hdr.sip_meta.v_3; }
  hdr.sip_meta.setInvalid();
 }

 action do_not_recirc_end_in_eg(bit<8> next_round){
  // route_to((bit<9>)hdr.sip_meta.dest_port);
  hdr.sip_meta.curr_round = next_round;
 }

 table tb_recirc_decision {
  key = {
   hdr.sip_meta.curr_round: exact;
  }
  actions = {
   incr_and_recirc;
   do_not_recirc_end_in_eg;
   do_not_recirc_end_in_ig;
   NoAction;
  }
  size = 32;
  default_action = NoAction();
  const entries = {
   // ingress performs round 0,4,8,...
   // even NUM_WORDS last round ends in egress, odd ends in ingress
    (0*4): incr_and_recirc(0*4+2); 
    (1*4): incr_and_recirc(1*4+2);
    (4*2): do_not_recirc_end_in_eg(4*2+2);
  }
 }
 action sip_init(bit<32> key_0, bit<32> key_1){
  hdr.sip_meta.v_0 = key_0 ^ const_0;
  hdr.sip_meta.v_1 = key_1 ^ const_1;
  hdr.sip_meta.v_2 = key_0 ^ const_2;
  hdr.sip_meta.v_3 = key_1 ^ const_3;
}

action sip_1_odd(){
  //for first SipRound in set of <c> SipRounds
  //i_3 = i_3 ^ message
  hdr.sip_meta.v_3 = hdr.sip_meta.v_3 ^ meta.sip_tmp.i_0;
}

action sip_1_a(){
  //a_0 = i_0 + i_1
  meta.sip_tmp.a_0 = hdr.sip_meta.v_0 + hdr.sip_meta.v_1;
  //a_2 = i_2 + i_3
  meta.sip_tmp.a_2 = hdr.sip_meta.v_2 + hdr.sip_meta.v_3;
  //a_1 = i_1 << 5
  @in_hash { meta.sip_tmp.a_1 = hdr.sip_meta.v_1[26:0] ++ hdr.sip_meta.v_1[31:27]; }
}
action sip_1_b(){
  //a_3 = i_3 << 8
  meta.sip_tmp.a_3 = hdr.sip_meta.v_3[23:0] ++ hdr.sip_meta.v_3[31:24];
}
action sip_2_a(){
  //b_1 = a_1 ^ a_0
  meta.sip_tmp.i_1 = meta.sip_tmp.a_1 ^ meta.sip_tmp.a_0;
  //b_3 = a_3 ^ a_2
  meta.sip_tmp.i_3 = meta.sip_tmp.a_3 ^ meta.sip_tmp.a_2;
  // b_0 = a_0 << 16
  meta.sip_tmp.i_0 = meta.sip_tmp.a_0[15:0] ++ meta.sip_tmp.a_0[31:16];
  //b_2 = a_2
  meta.sip_tmp.i_2 = meta.sip_tmp.a_2;
}

action sip_3_a(){
  //c_2 = b_2 + b_1
  meta.sip_tmp.a_2 = meta.sip_tmp.i_2 + meta.sip_tmp.i_1;
  //c_0 = b_0 + b_3
  meta.sip_tmp.a_0 = meta.sip_tmp.i_0 + meta.sip_tmp.i_3;
  //c_1 = b_1 << 13
  @in_hash { meta.sip_tmp.a_1 = meta.sip_tmp.i_1[18:0] ++ meta.sip_tmp.i_1[31:19]; }
 }
 action sip_3_b(){
  //c_3 = b_3 << 7
  @in_hash { meta.sip_tmp.a_3 = meta.sip_tmp.i_3[24:0] ++ meta.sip_tmp.i_3[31:25]; }
 }

action sip_4_a(){
  //d_1 = c_1 ^ c_2
  hdr.sip_meta.v_1 = meta.sip_tmp.a_1 ^ meta.sip_tmp.a_2;
  //d_3 = c_3 ^ c_0 i
  hdr.sip_meta.v_3 = meta.sip_tmp.a_3 ^ meta.sip_tmp.a_0;
  //d_2 = c_2 << 16
  hdr.sip_meta.v_2 = meta.sip_tmp.a_2[15:0] ++ meta.sip_tmp.a_2[31:16];

}
action sip_4_b_odd(){
  //d_0 = c_0
  hdr.sip_meta.v_0 = meta.sip_tmp.a_0;
}
action sip_4_b_even(){
  //d_0 = c_0 ^ message
  hdr.sip_meta.v_0 = meta.sip_tmp.a_0 ^ meta.sip_tmp.i_0;
}

 //compression rounds
 // round 0~(2*NUM_WORDS-1)
action start_m_0_compression(){ meta.sip_tmp.round_type = 0; meta.sip_tmp.i_0 = hdr.sip.m_0; } 
action start_m_1_compression(){ meta.sip_tmp.round_type = 0; meta.sip_tmp.i_0 = hdr.sip.m_1; } 
action start_m_2_compression(){ meta.sip_tmp.round_type = 0; meta.sip_tmp.i_0 = hdr.sip.m_2; } 
action start_m_3_compression(){ meta.sip_tmp.round_type = 0; meta.sip_tmp.i_0 = hdr.sip.m_3; }

 //round 2*NUM_WORDS (first 2 finalization rounds)
 action start_finalization_a(){
  meta.sip_tmp.round_type = 1;
  meta.sip_tmp.i_0 = 0;
  // also xor v2 with FF at beginning of first finalization pass
  hdr.sip_meta.v_2 = hdr.sip_meta.v_2 ^ 32w0xff;
 }
 //round 2*NUM_WORDS+2 (last 2 finalization rounds)
 action start_finalization_b(){
  meta.sip_tmp.round_type = 2;
  meta.sip_tmp.i_0 = 0;
 }

 table tb_start_round {
  key = {
   hdr.sip_meta.curr_round: exact;
  }
  size = 32;
  actions = {
    start_m_0_compression;
    start_m_2_compression;
    start_finalization_a;
  }
  const entries = {
    (0*2): start_m_0_compression();
    (2*2): start_m_2_compression();
    (4*2): start_finalization_a();
  }
 }

action pre_end_m_0_compression(){ meta.sip_tmp.i_0 = hdr.sip.m_0; } 
action pre_end_m_1_compression(){ meta.sip_tmp.i_0 = hdr.sip.m_1; } 
action pre_end_m_2_compression(){ meta.sip_tmp.i_0 = hdr.sip.m_2; } 
action pre_end_m_3_compression(){ meta.sip_tmp.i_0 = hdr.sip.m_3; }

action pre_end_finalization_a(){
  meta.sip_tmp.i_0 = 0;
}
action pre_end_finalization_b(){
  meta.sip_tmp.i_0 = 0;
}

table tb_pre_end{
key = {
  hdr.sip_meta.curr_round: exact;
}
size = 32;
actions = {
  pre_end_m_0_compression; 
  pre_end_m_2_compression;
  pre_end_finalization_a;
}
  const entries = {
      (0*2): pre_end_m_0_compression(); 
      (2*2): pre_end_m_2_compression();
      (4*2): pre_end_finalization_a();
  }
}

action start_first_pass(){
  //first pass init
  hdr.sip_meta.setValid();
  hdr.sip_meta.curr_round=0;

  sip_init(SIP_KEY_0, SIP_KEY_1);
  start_m_0_compression();
  // routing_decision();
}