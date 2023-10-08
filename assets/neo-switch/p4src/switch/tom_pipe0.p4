#include "headers_pipe0.p4"
#include "parsers_pipe0.p4"

control TomPipe0SwitchIngress(
    inout pipe_0_header_t hdr,
    inout pipe_0_ig_metadata_t ig_md,
    in ingress_intrinsic_metadata_t ig_intr_md,
    in ingress_intrinsic_metadata_from_parser_t ig_intr_prsr_md,
    inout ingress_intrinsic_metadata_for_deparser_t ig_intr_dprsr_md,
    inout ingress_intrinsic_metadata_for_tm_t ig_intr_tm_md){
    
    action drop(){
		ig_intr_dprsr_md.drop_ctl = 0x1;
	}

    action route_to(bit<9> port){
		ig_intr_tm_md.ucast_egress_port = port;
	}

    // Register<bit<8>, _> (32w1) session;
    // RegisterAction<bit<8>, _, bit<8>>(session) get_session = {
    //     void apply(inout bit<8> val, out bit<8> rv) {
    //         rv = val;
    //     }
    // };

    Register<bit<1>, _> (32w1) active;
    RegisterAction<bit<1>, _, bit<1>>(active) get_status = {
        void apply(inout bit<1> val, out bit<1> rv) {
            rv = val;
        }
    };

    Register<bit<8>, _> (32w1) session;
    RegisterAction<bit<8>, _, bit<8>>(session) get_session = {
        void apply(inout bit<8> val, out bit<8> rv) {
            rv = val;
        }
    };

    Register<bit<32>, _> (32w1) counter;
    RegisterAction<bit<32>, _, bit<32>>(counter) counter_update = {
        void apply(inout bit<32> val, out bit<32> rv) {
            val = val + 1;
            rv = val;
        }
    };

    table do_select_recirculation_port {
        key = {
            hdr.bft.pad0 : exact;
        }
        actions = {
            route_to;
            NoAction;
        }
        const entries = {
#ifdef DEBUG
            // TODO: Put all pipe 1 ports in loopback mode
            8w0 : route_to(9w128);
            8w1 : route_to(9w132);
            8w2 : route_to(9w136);
            8w3 : route_to(9w140);
            8w4 : route_to(9w144);
            8w5 : route_to(9w148);
            8w6 : route_to(9w152);
            8w7 : route_to(9w156);
            8w8 : route_to(9w160);
            8w9 : route_to(9w164);
            8w10 : route_to(9w168);
            8w11 : route_to(9w172);
            8w12 : route_to(9w176);
            8w13 : route_to(9w180);
            8w14 : route_to(9w184);
            8w15 : route_to(9w188);
#else
            8w0 : route_to(9w192);  // shard0 - use 192
            8w1 : route_to(9w196);  // shard1 - use 196
#endif
        }
        default_action = NoAction();
        size = 16;
    }

#define BW 4
#ifdef DEBUG
    Random<bit<BW>>() rand;
#endif 

    #include "copy.p4"
    #include "forwarding.p4"

    apply {
        bit<1> is_active = get_status.execute(0);
        if(is_active == 0) {
            drop();
            exit;
        }

        if(hdr.bft.isValid()) {
            hdr.udp.checksum = 0;

            // packet first entered the switch
            // stamp the packet session and message number
            if(hdr.bft.pad0 == 0 && hdr.bft.pad1 == 0) {
                hdr.bft.sess_num = get_session.execute(0);
                hdr.bft.msg_num = counter_update.execute(0);

                // take what we need from the digest
                // hdr.s_digest.setValid();
                // copy_s_digest();
                // hdr.digest.setInvalid();
            }

            // init the headers siphash, siphash_meta
            if(do_init_sip_hash.apply().hit) {
                // copy the headers for siphash
                copy_sip00();
                copy_sip01();
                copy_sip10();
                copy_sip11();

#ifdef DEBUG
                hdr.bft.pad0 = (bit<8>) rand.get();
#endif

                // depending on shard number, select the corresponding recirc port
                do_select_recirculation_port.apply();

#ifdef DEBUG
                hdr.bft.pad0 = 0;
#endif

                // set the packet state to recirc 
                hdr.bft.pad0 = hdr.bft.pad0 + 128;
            } 

#ifdef MEASURE_LATENCY
            hdr.ethernet.src_addr = ig_intr_prsr_md.global_tstamp;
#endif 

        } else {
            l2_forwarding.apply();
            ig_intr_tm_md.bypass_egress = 1;
        }
    }

}

control TomPipe0SwitchEgress(
    inout pipe_0_header_t hdr,
    inout pipe_0_eg_metadata_t eg_md,
    in egress_intrinsic_metadata_t eg_intr_md,
    in egress_intrinsic_metadata_from_parser_t eg_intr_md_from_prsr,
    inout egress_intrinsic_metadata_for_deparser_t eg_intr_dprsr_md,
    inout egress_intrinsic_metadata_for_output_port_t eg_intr_oport_md) {
    
    action drop(){
       eg_intr_dprsr_md.drop_ctl = 0x1; // Drop packet.
	}

    action prepare_recirc() {
        hdr.sip00.setInvalid();
        hdr.sip01.setInvalid();
        hdr.sip10.setInvalid();
        hdr.sip11.setInvalid();

        hdr.bft.pad0 = hdr.bft.pad0 + 1;    // signal next shard
        hdr.bft.pad1 = 0;
    }

    action no_recirc() {
        // hdr.out0.setValid();
        // hdr.out1.setValid();
        // hdr.out2.setValid();
        // hdr.out3.setValid();
        
        hdr.out0.h_0 = hdr.sip00.m_0;
        hdr.out1.h_0 = hdr.sip01.m_0;
        hdr.out2.h_0 = hdr.sip10.m_0;
        hdr.out3.h_0 = hdr.sip11.m_0;

        hdr.sip00.setInvalid();
        hdr.sip01.setInvalid();
        hdr.sip10.setInvalid();
        hdr.sip11.setInvalid();

        hdr.bft.pad0 = 0;
        hdr.bft.pad1 = 0;
        // hdr.s_digest.setInvalid();
    }

    table do_recirc {
        key = {
            eg_intr_md.egress_port : exact;
        }
        actions = {
            prepare_recirc;
            no_recirc;
        }
        const entries = {
            68 : prepare_recirc;
        }
        default_action = no_recirc();
        size = 128;
    }

#ifdef MEASURE_LATENCY
    bit<32> temp0 = 0;
    bit<32> temp1 = 0;
    bit<32> temp2 = 0;
#endif 

    Register<bit<32>, _> (32w1) counter;
    RegisterAction<bit<32>, _, bit<32>>(counter) counter_update = {
        void apply(inout bit<32> val, out bit<32> rv) {
            val = val + 1;
            rv = val;
        }
    };

    apply {
        if(hdr.bft.isValid()) {
            if(hdr.bft.pad1 == 128) {   // this indiciates SipHash done
                
                counter_update.execute(0);

#ifdef MEASURE_LATENCY
                temp0 = hdr.ethernet.src_addr[31:0];
                temp1 = eg_intr_md_from_prsr.global_tstamp[31:0];
                temp2 = temp1 - temp0;
                hdr.ethernet.src_addr = 16w0 ++ temp2;
#endif 
               do_recirc.apply();
            }
        }
    }
}