#ifndef SIP_KEY_00_0
#define SIP_KEY_00_0 0x33323130
#endif

#ifndef SIP_KEY_00_1
#define SIP_KEY_00_1 0x42413938
#endif
    
action sip00_incr_and_recirc(bit<8> next_round){
    hdr.sip00_meta.curr_round = next_round;
}

Hash<bit<32>>(HashAlgorithm_t.IDENTITY) sip00_copy32_output;

action sip00_do_not_recirc(){
    bit<32> hash_output = hdr.sip00_meta.v_0 ^ hdr.sip00_meta.v_1 ^ hdr.sip00_meta.v_2 ^ hdr.sip00_meta.v_3;

    hdr.sip00.m_0 = 0; 
    hdr.sip00.m_1 = 0; 
    hdr.sip00.m_2 = 0; 
    hdr.sip00.m_3 = 0;
    hdr.sip00.m_0=sip00_copy32_output.get({hash_output});
    hdr.sip00_meta.setInvalid();
}

table sip00_tb_recirc_decision {
    key = {
        hdr.sip00_meta.curr_round: exact;
    }
    actions = {
        sip00_incr_and_recirc;
        sip00_do_not_recirc;
        NoAction;
    }
    size = 20;
    default_action = NoAction();
    const entries = {
        // two rounds per pass. #passes=(NUM_WORDS+2), need to recirculate NUM_WORDS+1 times.
        (0): sip00_incr_and_recirc(1);
        (1): sip00_incr_and_recirc(2);
        (2): sip00_incr_and_recirc(3);
        (3): sip00_incr_and_recirc(4);
        (4): sip00_incr_and_recirc(5);
        (5): sip00_incr_and_recirc(6);
        (6): sip00_incr_and_recirc(7);
        (7): sip00_incr_and_recirc(8);
        (8): sip00_incr_and_recirc(9);
        (9): sip00_incr_and_recirc(10);
        (10): sip00_incr_and_recirc(11);
        (11): sip00_do_not_recirc();
    }
}

action sip00_init(bit<32> key_0, bit<32> key_1){
    hdr.sip00_meta.v_0 = key_0 ^ const_0;
    hdr.sip00_meta.v_1 = key_1 ^ const_1;
    hdr.sip00_meta.v_2 = key_0 ^ const_2;
    hdr.sip00_meta.v_3 = key_1 ^ const_3;
}
Hash<bit<32>>(HashAlgorithm_t.IDENTITY) sip00_copy32_a_1;
Hash<bit<32>>(HashAlgorithm_t.IDENTITY) sip00_copy32_a_3;
Hash<bit<32>>(HashAlgorithm_t.IDENTITY) sip00_copy32_b_0;
Hash<bit<32>>(HashAlgorithm_t.IDENTITY) sip00_copy32_c_1;
Hash<bit<32>>(HashAlgorithm_t.IDENTITY) sip00_copy32_c_3;
Hash<bit<32>>(HashAlgorithm_t.IDENTITY) sip00_copy32_d_2;

action sip00_1_odd(){
    //for first SipRound in set of <c> SipRounds
    //i_3 = i_3 ^ message
    hdr.sip00_meta.v_3 = hdr.sip00_meta.v_3 ^ eg_md.sip00_tmp.i_0;
}
action sip00_1_a(){
    //a_0 = i_0 + i_1
    eg_md.sip00_tmp.a_0 = hdr.sip00_meta.v_0 + hdr.sip00_meta.v_1;
    //a_2 = i_2 + i_3
    eg_md.sip00_tmp.a_2 = hdr.sip00_meta.v_2 + hdr.sip00_meta.v_3;
    //a_1 = i_1 << 5
    eg_md.sip00_tmp.a_1 = sip00_copy32_a_1.get({hdr.sip00_meta.v_1[26:0] ++ hdr.sip00_meta.v_1[31:27]});
}
action sip00_1_b(){
    //a_3 = i_3 << 8
    eg_md.sip00_tmp.a_3 = sip00_copy32_a_3.get({hdr.sip00_meta.v_3[23:0] ++ hdr.sip00_meta.v_3[31:24]});
}
action sip00_2_a(){
    //b_1 = a_1 ^ a_0
    eg_md.sip00_tmp.i_1 = eg_md.sip00_tmp.a_1 ^ eg_md.sip00_tmp.a_0;
    //b_3 = a_3 ^ a_2
    eg_md.sip00_tmp.i_3 = eg_md.sip00_tmp.a_3 ^ eg_md.sip00_tmp.a_2;
    //b_0 = a_0 << 16
    eg_md.sip00_tmp.i_0 = sip00_copy32_b_0.get({eg_md.sip00_tmp.a_0[15:0] ++ eg_md.sip00_tmp.a_0[31:16]});
    //b_2 = a_2
    eg_md.sip00_tmp.i_2 = eg_md.sip00_tmp.a_2;
}

action sip00_3_a(){
    //c_2 = b_2 + b_1
    eg_md.sip00_tmp.a_2 = eg_md.sip00_tmp.i_2 + eg_md.sip00_tmp.i_1;
    //c_0 = b_0 + b_3
    eg_md.sip00_tmp.a_0 = eg_md.sip00_tmp.i_0 + eg_md.sip00_tmp.i_3;
    //c_1 = b_1 << 13
    eg_md.sip00_tmp.a_1 = sip00_copy32_c_1.get({eg_md.sip00_tmp.i_1[18:0] ++ eg_md.sip00_tmp.i_1[31:19]});
}
action sip00_3_b(){
    //c_3 = b_3 << 7
    eg_md.sip00_tmp.a_3 = sip00_copy32_c_3.get({eg_md.sip00_tmp.i_3[24:0] ++ eg_md.sip00_tmp.i_3[31:25]});
}

action sip00_4_a(){
    //d_1 = c_1 ^ c_2
    hdr.sip00_meta.v_1 = eg_md.sip00_tmp.a_1 ^ eg_md.sip00_tmp.a_2;
    //d_3 = c_3 ^ c_0 i
    hdr.sip00_meta.v_3 = eg_md.sip00_tmp.a_3 ^ eg_md.sip00_tmp.a_0;
    //d_2 = c_2 << 16
    hdr.sip00_meta.v_2 = sip00_copy32_d_2.get({eg_md.sip00_tmp.a_2[15:0] ++ eg_md.sip00_tmp.a_2[31:16]});
}
action sip00_4_b_odd(){
    //d_0 = c_0
    hdr.sip00_meta.v_0 = eg_md.sip00_tmp.a_0;
}
action sip00_4_b_even(){
    //d_0 = c_0 ^ message
    hdr.sip00_meta.v_0 = eg_md.sip00_tmp.a_0 ^ eg_md.sip00_tmp.i_0;
}
// round 0~(2*NUM_WORDS-1)

action sip00_start_m_0_compression(){ 
    eg_md.sip00_tmp.round_type = 0; 
    eg_md.sip00_tmp.i_0 = hdr.sip00.m_0; 
} 
action sip00_start_m_1_compression(){ 
    eg_md.sip00_tmp.round_type = 0; 
    eg_md.sip00_tmp.i_0 = hdr.sip00.m_1; 
} 
action sip00_start_m_2_compression(){ 
    eg_md.sip00_tmp.round_type = 0; 
    eg_md.sip00_tmp.i_0 = hdr.sip00.m_2; 
} 
action sip00_start_m_3_compression(){ 
    eg_md.sip00_tmp.round_type = 0; 
    eg_md.sip00_tmp.i_0 = hdr.sip00.m_3; 
}

//round 2*NUM_WORDS (first 2 finalization rounds)
action sip00_start_finalization_a(){
    eg_md.sip00_tmp.round_type = 1;
    eg_md.sip00_tmp.i_0 = 0;
    // also xor v2 with FF at beginning of the first finalization pass
    hdr.sip00_meta.v_2 = hdr.sip00_meta.v_2 ^ 32w0xff;
}
//round 2*NUM_WORDS+2 (last 2 finalization rounds)
action sip00_start_finalization_b(){
    eg_md.sip00_tmp.round_type = 2;
    eg_md.sip00_tmp.i_0 = 0;
}

table sip00_tb_start_round {
    key = {
        hdr.sip00_meta.curr_round: exact;
    }
    size = 32;
    actions = {
        sip00_start_m_0_compression; 
        sip00_start_m_1_compression; 
        sip00_start_m_2_compression; 
        sip00_start_m_3_compression;
        sip00_start_finalization_a;
        sip00_start_finalization_b;
    }
    const entries = {
        // note: (0) is actually handled by sip00_start_first_pass()
        (0*2): sip00_start_m_0_compression(); 
        (1*2): sip00_start_m_1_compression(); 
        (2*2): sip00_start_m_2_compression(); 
        (3*2): sip00_start_m_3_compression();
        (4*2): sip00_start_finalization_a();
        (4*2+2): sip00_start_finalization_b();
    }
}

action sip00_pre_end_m_0_compression(){ eg_md.sip00_tmp.i_0 = hdr.sip00.m_0; } 
action sip00_pre_end_m_1_compression(){ eg_md.sip00_tmp.i_0 = hdr.sip00.m_1; } 
action sip00_pre_end_m_2_compression(){ eg_md.sip00_tmp.i_0 = hdr.sip00.m_2; } 
action sip00_pre_end_m_3_compression(){ eg_md.sip00_tmp.i_0 = hdr.sip00.m_3; }

action sip00_pre_end_finalization_a(){
    eg_md.sip00_tmp.i_0 = 0;
}
action sip00_pre_end_finalization_b(){
    eg_md.sip00_tmp.i_0 = 0;
}

table sip00_tb_pre_end{
    key = {
        hdr.sip00_meta.curr_round: exact;
    }
    size = 32;
    actions = {
        sip00_pre_end_m_0_compression; 
        sip00_pre_end_m_1_compression; 
        sip00_pre_end_m_2_compression;
        sip00_pre_end_m_3_compression;
        sip00_pre_end_finalization_a;
        sip00_pre_end_finalization_b;
    }
    const entries = {
        (1): sip00_pre_end_m_0_compression(); 
        (3): sip00_pre_end_m_1_compression(); 
        (5): sip00_pre_end_m_2_compression(); 
        (7): sip00_pre_end_m_3_compression();
        (9): sip00_pre_end_finalization_a();
        (11): sip00_pre_end_finalization_b();
    }
}

action sip00_start_first_pass(){
    //first pass init
    hdr.sip00_meta.setValid();
    hdr.sip00_meta.curr_round=0;

    sip00_init(SIP_KEY_00_0, SIP_KEY_00_1);
    sip00_start_m_0_compression();
}

table sip00_tb_odd_even {
    key = {
        hdr.sip00_meta.curr_round: exact;
    }
    size = 32;
    actions = {
        sip00_4_b_even;
        sip00_4_b_odd;
    }
    const entries = {
        (0): sip00_4_b_odd();
        (1): sip00_4_b_even();
        (2): sip00_4_b_odd();
        (3): sip00_4_b_even();
        (4): sip00_4_b_odd();
        (5): sip00_4_b_even();
        (6): sip00_4_b_odd();
        (7): sip00_4_b_even();
        (8): sip00_4_b_odd();
        (9): sip00_4_b_even();
        (10): sip00_4_b_odd();
        (11): sip00_4_b_even();
    }
}
