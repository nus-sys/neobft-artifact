action broadcast() {
    ig_intr_tm_md.mcast_grp_a       = 999;
    ig_intr_tm_md.level2_exclusion_id = ig_intr_md.ingress_port;
}

action l2_forward(PortId_t port) {
    ig_intr_tm_md.ucast_egress_port=port;
}

table l2_forwarding {
    key = {
        hdr.ethernet.dst_addr : exact;
    }
    actions = {
        drop;
        broadcast;
        l2_forward;
    }
    const entries = {
        0xb8cef62a2f94 : l2_forward(0);
        0xb8cef62a45fc : l2_forward(4);
        0xb8cef62a3f9c : l2_forward(8);
        0xb8cef62a30ec : l2_forward(12);
        0x649d99b1688e : l2_forward(16);
        0x649d99b1669a : l2_forward(20);
        0x08c0ebb6e7e4 : l2_forward(36);
        0xffffffffffff : broadcast();
    }
    default_action = drop();
    size = 128;
}