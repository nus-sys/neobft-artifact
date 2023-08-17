#include "headers_pipe0.p4"
#include "parsers_pipe0.p4"

control TomPipe0SwitchIngress(
    /* User */
    inout pipe0_ingress_headers_t                       hdr,
    inout pipe0_ingress_metadata_t                      meta,
    /* Intrinsic */
    in    ingress_intrinsic_metadata_t               ig_intr_md,
    in    ingress_intrinsic_metadata_from_parser_t   ig_prsr_md,
    inout ingress_intrinsic_metadata_for_deparser_t  ig_dprsr_md,
    inout ingress_intrinsic_metadata_for_tm_t        ig_tm_md)
{

    bit<32> avail_hash = 0;
    bit<32> prev_hash = 0;
    bit<32> avail_accel = 0;
    bit<32> avail_pipe = 0;

    action drop(){
        ig_dprsr_md.drop_ctl = 0x1; 
	}

    action multicast(MulticastGroupId_t mcast_grp) {
        ig_tm_md.mcast_grp_a       = mcast_grp;
        ig_tm_md.level2_exclusion_id = 0xFF;
    }

    action mirror_to_accel(bit<10> mirror_session){
        meta.mirror_session = mirror_session;
        ig_dprsr_md.mirror_type = MIRROR_TYPE_ACCEL;
    }


    #include "regs.p4"
    #include "copy.p4"
    #include "forwarding.p4"

    apply {
        // if(hdr.udp.isValid()) {
        //     hdr.udp.checksum = 0;
        // }
        if(ig_intr_md.ingress_port == PIPE_0_COPY && hdr.bft.isValid() && hdr.udp.dst_port == BFT_COPY) {
            // multicast to replica and COPY port
            multicast(888);

        } else if(ig_intr_md.ingress_port == ACCEL_PORT && hdr.bft.isValid()) {
            // if packet is received from the accelerator
            // free accelerator resources
            // free_accel.execute(0);
            // multicast signed packet to replicas
            // multicast(998);
            
            // multicast to replica and COPY port
            multicast(888);
        } else if(hdr.sip.isValid() && hdr.bft.isValid()) {
            // free pipe and allow next message to process
            // free_pipe.execute(0);

            // siphash is done
            // update the registers with the new hash
            // @stage(1){
            update_pkt_hash.execute(0);
            update_pkt_hash_counter.execute(0);
            // }

            hdr.sip.setInvalid();
            hdr.udp.dst_port = BFT_PORT;

            // check accelerator availability
            // avail_accel = get_accel.execute(0);
            // if(avail_accel != 0) {
            //     // send to accelerator for signature
            //     ig_tm_md.ucast_egress_port = ACCEL_PORT;

            //     // mirror to accelerator for signature
            //     // mirror_to_accel(10w128);

            //     // multicast to replicas
            //     // multicast(998);
            // } else {
            //     // no need to send to accelerator
            //     // multicast to replicas
            //     multicast(998);
            //     // ig_tm_md.ucast_egress_port = 1; // testing
            // }

            // just mirror to accelerator
            // mirror_to_accel(10w128);
            // multicast(997);

            // multicast to replica, smartNIC and COPY port
            multicast(887);
        } 
        
        // else if(hdr.udp.dst_port == BFT_PORT && hdr.bft.isValid()) {
        //     // first, check whether pipe is available
        //     avail_pipe = get_pipe.execute(0);
        //     if(avail_pipe == 1) {
        //         hdr.bft.sess_num = get_session.execute(0);
        //         hdr.bft.msg_num = increment_msg_num.execute(0);

        //         prev_hash = get_pkt_hash.execute(0);
        //         avail_hash = get_pkt_hash_counter.execute(0);

        //         hdr.bft.prev_hash = prev_hash;
        //         hdr.sip.setValid();
        //         copy_sip();

        //         ig_tm_md.ucast_egress_port = PIPE_1_RECIRC_0;
        //     } else {
        //         drop();
        //         exit;
        //     }
        // }
        
        else if(hdr.bft.isValid()) {
            // stamp sess_num and msg_num for every new packet
            if(hdr.udp.dst_port == BFT_PORT) {
                // first, check whether pipe is available
                // avail_pipe = get_pipe.execute(0);
                // if(avail_pipe == 0) {
                //     drop();
                //     exit;
                // }

                hdr.bft.sess_num = get_session.execute(0);
                hdr.bft.msg_num = increment_msg_num.execute(0);
            } 
            
            // check whether the hash for the next packet is available
            // @stage(1){
            prev_hash = get_pkt_hash.execute(0);
            avail_hash = get_pkt_hash_counter.execute(0);
            // }
            
            avail_hash = avail_hash + 1;
            if(hdr.bft.msg_num == avail_hash) {
                // hash available
                hdr.bft.prev_hash = prev_hash;

                // init sip headers and copy
                hdr.sip.setValid();
                copy_sip();

                // forward to pipe 1 to perform hashing
                hdr.udp.dst_port = BFT_PORT;
                ig_tm_md.ucast_egress_port= PIPE_1_RECIRC_0;
            } else {
                // hash not yet available, continue waiting via recirculation
                hdr.udp.dst_port = BFT_HOLD;

                // ig_tm_md.ucast_egress_port = PIPE_0_RECIRC;

                // use different class to buffer these traffic
                ig_tm_md.ucast_egress_port = PIPE_0_HOLDING;
                ig_tm_md.ingress_cos = 3w6; // special icos to map to lossless PPG
            }
        } 
        
        else {
            l2_forwarding.apply();
        }
    }
}

control TomPipe0SwitchEgress(
    /* User */
    inout pipe0_egress_headers_t                          hdr,
    inout pipe0_egress_metadata_t                         meta,
    /* Intrinsic */
    in    egress_intrinsic_metadata_t                  eg_intr_md,
    in    egress_intrinsic_metadata_from_parser_t      eg_prsr_md,
    inout egress_intrinsic_metadata_for_deparser_t     eg_dprsr_md,
    inout egress_intrinsic_metadata_for_output_port_t  eg_oport_md)
{
    action drop(){
       eg_dprsr_md.drop_ctl = 0x1; // Drop packet.
	}

    action mod_dst_ip_and_port_action(bit<32> dst_ip, bit<16> dst_port, bit<48> dst_addr) {
        hdr.ethernet.dst_addr = dst_addr;
	hdr.ipv4.dst_addr = dst_ip;
        hdr.udp.dst_port = dst_port;
        hdr.udp.checksum = 0;
    }

    table mod_dst_ip_and_port {
        key = {
            eg_intr_md.egress_port : exact;
            hdr.udp.checksum : exact;
        }
        actions = {
            drop;
            mod_dst_ip_and_port_action;
            NoAction;
        }
        const entries = {
            // node1
           (0, 0) : mod_dst_ip_and_port_action(0x0b000001, 1102, 0xb8cef62a2f94);
           (0, 1) : mod_dst_ip_and_port_action(0x0b000001, 1103, 0xb8cef62a2f94);
           (0, 2) : mod_dst_ip_and_port_action(0x0b000001, 1104, 0xb8cef62a2f94);
           (0, 3) : mod_dst_ip_and_port_action(0x0b000001, 1105, 0xb8cef62a2f94);
           (0, 4) : mod_dst_ip_and_port_action(0x0b000001, 1106, 0xb8cef62a2f94);
           (0, 5) : mod_dst_ip_and_port_action(0x0b000001, 1107, 0xb8cef62a2f94);
           // node2
           (4, 0) : mod_dst_ip_and_port_action(0x0b000002, 1102, 0xb8cef62a45fc);
           (4, 1) : mod_dst_ip_and_port_action(0x0b000002, 1103, 0xb8cef62a45fc);
           (4, 2) : mod_dst_ip_and_port_action(0x0b000002, 1104, 0xb8cef62a45fc);
           (4, 3) : mod_dst_ip_and_port_action(0x0b000002, 1105, 0xb8cef62a45fc);
           (4, 4) : mod_dst_ip_and_port_action(0x0b000002, 1106, 0xb8cef62a45fc);
           (4, 5) : mod_dst_ip_and_port_action(0x0b000002, 1107, 0xb8cef62a45fc);
           // node3
           (8, 0) : mod_dst_ip_and_port_action(0x0b000003, 1102, 0xb8cef62a3f9c);
           (8, 1) : mod_dst_ip_and_port_action(0x0b000003, 1103, 0xb8cef62a3f9c);
           (8, 2) : mod_dst_ip_and_port_action(0x0b000003, 1104, 0xb8cef62a3f9c);
           (8, 3) : mod_dst_ip_and_port_action(0x0b000003, 1105, 0xb8cef62a3f9c);
           (8, 4) : mod_dst_ip_and_port_action(0x0b000003, 1106, 0xb8cef62a3f9c);
           (8, 5) : mod_dst_ip_and_port_action(0x0b000003, 1107, 0xb8cef62a3f9c);
           // node4
           (12, 0) : mod_dst_ip_and_port_action(0x0b000004, 1102, 0xb8cef62a30ec);
           (12, 1) : mod_dst_ip_and_port_action(0x0b000004, 1103, 0xb8cef62a30ec);
           (12, 2) : mod_dst_ip_and_port_action(0x0b000004, 1104, 0xb8cef62a30ec);
           (12, 3) : mod_dst_ip_and_port_action(0x0b000004, 1105, 0xb8cef62a30ec);
           (12, 4) : mod_dst_ip_and_port_action(0x0b000004, 1106, 0xb8cef62a30ec);
           (12, 5) : mod_dst_ip_and_port_action(0x0b000004, 1107, 0xb8cef62a30ec);
        }
        default_action = NoAction();
        size = 1024;
    }

    apply {
        if(hdr.bft.isValid()) {
            // two different scenarios
            if(hdr.udp.dst_port == BFT_PORT) {
                // freshly signed by NIC/ switch with ports 22222

                hdr.udp.checksum = 0;

                if(eg_intr_md.egress_port == ACCEL_PORT) {
                    // to smartNIC
                    // do nothing
                } else if(eg_intr_md.egress_port == PIPE_0_COPY) {
                    // to recirc port for response replication
                    hdr.udp.checksum = hdr.udp.checksum + 1;
                    hdr.udp.dst_port = BFT_COPY;
                } else {
                    // for the replicas
                    mod_dst_ip_and_port.apply();
                }
            } else if(hdr.udp.dst_port == BFT_COPY) {
                // in the middle of replicating
                if(eg_intr_md.egress_port == PIPE_0_COPY) {
                    // to recirc port for response replication
                    if(hdr.udp.checksum == 5) {
                        drop();
                    } else {
                        hdr.udp.checksum = hdr.udp.checksum + 1;
                        hdr.udp.dst_port = BFT_COPY;
                    }
                } else {
                    // for the replicas
                    mod_dst_ip_and_port.apply();
                }
            }
        }
    }
}
