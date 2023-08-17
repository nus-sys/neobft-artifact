#include "headers_pipe1.p4"
#include "parsers_pipe1.p4"

control TomPipe1SwitchIngress(
    /* User */
    inout pipe1_ingress_headers_t                       hdr,
    inout pipe1_ingress_metadata_t                      meta,
    /* Intrinsic */
    in    ingress_intrinsic_metadata_t               ig_intr_md,
    in    ingress_intrinsic_metadata_from_parser_t   ig_prsr_md,
    inout ingress_intrinsic_metadata_for_deparser_t  ig_dprsr_md,
    inout ingress_intrinsic_metadata_for_tm_t        ig_tm_md)
{

    action drop(){
        ig_dprsr_md.drop_ctl = 0x1; 
	}

    #include "sip_ingress.p4"

    apply {
        if(!hdr.sip_meta.isValid()){
            exit;
        } else {
            tb_start_round.apply();
        }
        
        //compression round: xor msg
        //note: for finalization rounds msg is zero, no effect//v3^=m
        sip_1_odd();
        //first SipRound
        sip_1_a();
        sip_1_b();
        sip_2_a();
        sip_3_a();
        sip_3_b();
        sip_4_a();
        sip_4_b_odd();
        //second SipRound
        sip_1_a();
        sip_1_b();
        sip_2_a();
        sip_3_a();
        sip_3_b();
        tb_pre_end.apply();
        sip_4_a();
        //v0^=m
        sip_4_b_even();

        if(hdr.sip_meta.curr_round < (4*2+2)){
            //need more rounds in ingress pipeline, packet should be during recirculation right now
            hdr.sip_meta.curr_round = hdr.sip_meta.curr_round + 2;
            ig_tm_md.ucast_egress_port = PIPE_1_RECIRC_0;
        } else{
            ig_tm_md.ucast_egress_port = PIPE_0_RECIRC;
            
            // TODO: port68, queue no7 with the highest priority to make sure that this will never be dropped
            // ig_tm_md.qid = 5w7;

            final_round_xor();
        }
    }
}

control TomPipe1SwitchEgress(
    /* User */
    inout pipe1_egress_headers_t                          hdr,
    inout pipe1_egress_metadata_t                         meta,
    /* Intrinsic */
    in    egress_intrinsic_metadata_t                  eg_intr_md,
    in    egress_intrinsic_metadata_from_parser_t      eg_prsr_md,
    inout egress_intrinsic_metadata_for_deparser_t     eg_dprsr_md,
    inout egress_intrinsic_metadata_for_output_port_t  eg_oport_md)
{

    action drop(){
        eg_dprsr_md.drop_ctl = 0x1; 
	}

    #include "sip_egress.p4"

    apply {
        // check for valid sip data
        bool is_sip = hdr.sip.isValid();
        if(!is_sip){
            drop();
            exit;
        } else{
            // logic check for first pass
            if(!hdr.sip_meta.isValid()){
                start_first_pass();
            } else {
                tb_start_round.apply();
            }
        }

        //compression round: xor msg
        //note: for finalization rounds msg is zero, no effect
        //v3^=m
        sip_1_odd();
        //first SipRound
        sip_1_a();
        sip_1_b();
        sip_2_a();
        sip_3_a();
        sip_3_b();
        sip_4_a();
        sip_4_b_odd();

        //second SipRound
        sip_1_a();
        sip_1_b();
        sip_2_a();
        sip_3_a();
        sip_3_b();
        tb_pre_end.apply();
        sip_4_a();
        //v0^=m
        sip_4_b_even();

        tb_recirc_decision.apply();
    }
}