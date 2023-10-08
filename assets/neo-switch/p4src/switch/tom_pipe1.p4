#include "headers_pipe1.p4"
#include "parsers_pipe1.p4"

control TomPipe1SwitchIngress(
    inout pipe_1_header_t hdr,
    inout pipe_1_ig_metadata_t ig_md,
    in ingress_intrinsic_metadata_t ig_intr_md,
    in ingress_intrinsic_metadata_from_parser_t ig_intr_prsr_md,
    inout ingress_intrinsic_metadata_for_deparser_t ig_intr_dprsr_md,
    inout ingress_intrinsic_metadata_for_tm_t ig_intr_tm_md){
    
    action drop(){
		ig_intr_dprsr_md.drop_ctl = 0x1;
	}
    action route_to(bit<9> port){
		ig_intr_tm_md.ucast_egress_port=port;
	}

    action multicast(MulticastGroupId_t mcast_grp) {
        ig_intr_tm_md.mcast_grp_a       = mcast_grp;
        ig_intr_tm_md.level2_exclusion_id = 0xFF;
    }

    table done_sip_and_multicast {
        key = {
            hdr.bft.pad0 : exact;
            hdr.bft.pad1 : exact;
        }
        actions = {
            multicast;
            route_to;
        }
        const entries = {
            // (128, 128) : multicast(990);    // shard0, port 1 & recirc port 68
            // (129, 128) : route_to(0); 
            (128, 128) : multicast(998);
            // (129, 11) : multicast(997); // shard 1
            // TODO: add more iterations
        }
        size = 128;
    }

    #include "sip10_ingress.p4"
    #include "sip11_ingress.p4"

    apply {
        if(hdr.sip10.isValid() && hdr.sip11.isValid()) {
            //logic check for first pass
            if(!hdr.sip10_meta.isValid()){
                sip10_start_first_pass();
            } else {
                sip10_tb_start_round.apply();
            }

            //logic check for first pass
            if(!hdr.sip11_meta.isValid()){
                sip11_start_first_pass();
            } else {
                sip11_tb_start_round.apply();
            }

            //compression round: xor msg
            //note: for finalization rounds msg is zero, no effect
            //v3^=m
            sip10_1_odd();
            //first SipRound
            sip10_1_a();
            sip10_1_b();
            sip10_2_a();
            sip10_3_a();
            sip10_3_b();
            sip10_tb_pre_end.apply();
            sip10_4_a();
            sip10_tb_odd_even.apply();
            sip10_tb_recirc_decision.apply();

            //compression round: xor msg
            //note: for finalization rounds msg is zero, no effect
            //v3^=m
            sip11_1_odd();
            // //first SipRound
            sip11_1_a();
            sip11_1_b();
            sip11_2_a();
            sip11_3_a();
            sip11_3_b();
            sip11_tb_pre_end.apply();
            sip11_4_a();
            sip11_tb_odd_even.apply();
            sip11_tb_recirc_decision.apply();

            // check whether we are done? then forward it back to pipe0
            // if(done_sip_and_multicast.apply().hit) {
            //     hdr.bft.pad0 = hdr.bft.pad0 - 128;  // clear recirc flag
            //     // hdr.bft.pad1 = 128;                 // signal siphash done
            // }
            if(hdr.bft.pad0 == 128 && hdr.bft.pad1 == 128) { // done with sip hash
                hdr.bft.pad0 = hdr.bft.pad0 - 128;
#ifdef DEBUG
                route_to(0); 
#else
                multicast(998);
#endif
            }
            else {
                route_to(ig_intr_md.ingress_port);  // continue to recirc
            }
        } else {
            drop();
        }
    }

}

control TomPipe1SwitchEgress(
    inout pipe_1_header_t hdr,
    inout pipe_1_eg_metadata_t eg_md,
    in egress_intrinsic_metadata_t eg_intr_md,
    in egress_intrinsic_metadata_from_parser_t eg_intr_md_from_prsr,
    inout egress_intrinsic_metadata_for_deparser_t eg_intr_dprsr_md,
    inout egress_intrinsic_metadata_for_output_port_t eg_intr_oport_md) {

    
    action drop(){
       eg_intr_dprsr_md.drop_ctl = 0x1; // Drop packet.
	}

    #include "sip00_egress.p4"
    #include "sip01_egress.p4"

    apply {
        if(hdr.sip00.isValid() && hdr.sip01.isValid()) {
            if(!hdr.sip00_meta.isValid()){
                sip00_start_first_pass();
            } else {
                sip00_tb_start_round.apply();
            }

            if(!hdr.sip01_meta.isValid()){
                sip01_start_first_pass();
            } else {
                sip01_tb_start_round.apply();
            }

            //compression round: xor msg
            //note: for finalization rounds msg is zero, no effect
            //v3^=m
            sip00_1_odd();
            //first SipRound
            sip00_1_a();
            sip00_1_b();
            sip00_2_a();
            sip00_3_a();
            sip00_3_b();
            sip00_tb_pre_end.apply();
            sip00_4_a();
            sip00_tb_odd_even.apply();
            sip00_tb_recirc_decision.apply();

            //compression round: xor msg
            //note: for finalization rounds msg is zero, no effect
            //v3^=m
            sip01_1_odd();
            //first SipRound
            sip01_1_a();
            sip01_1_b();
            sip01_2_a();
            sip01_3_a();
            sip01_3_b();
            sip01_tb_pre_end.apply();
            sip01_4_a();
            sip01_tb_odd_even.apply();
            sip01_tb_recirc_decision.apply();
        } else {
            drop();
        }
    }
}