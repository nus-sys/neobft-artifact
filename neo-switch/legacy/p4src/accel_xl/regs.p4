Register<bit<16>, _> (32w1) session;
RegisterAction<bit<16>, _, bit<16>>(session) get_session = {
    void apply(inout bit<16> val, out bit<16> rv) {
        rv = val;
    }
};

Register<bit<32>, _> (32w1) msg_num;
RegisterAction<bit<32>, _, bit<32>>(msg_num) increment_msg_num = {
    void apply(inout bit<32> val, out bit<32> rv) {
        val = val + 1;
        rv = val;
    }
};

Register<bit<32>, _> (32w1) pkt_hash;
RegisterAction<bit<32>, _, bit<32>>(pkt_hash) update_pkt_hash = {
    void apply(inout bit<32> val, out bit<32> rv) {
        rv = val;
        val = hdr.sip.m_0;    
    }
};
RegisterAction<bit<32>, _, bit<32>>(pkt_hash) get_pkt_hash = {
    void apply(inout bit<32> val, out bit<32> rv) {
        rv = val;
    }
};

Register<bit<32>, _> (32w1) pkt_hash_counter;
RegisterAction<bit<32>, _, bit<32>>(pkt_hash_counter) update_pkt_hash_counter = {
    void apply(inout bit<32> val, out bit<32> rv) {
        rv = val;
        val = hdr.bft.msg_num;    
    }
};
RegisterAction<bit<32>, _, bit<32>>(pkt_hash_counter) get_pkt_hash_counter = {
    void apply(inout bit<32> val, out bit<32> rv) {
        rv = val;
    }
};

Register<bit<32>, _> (32w1) accel_avail;
RegisterAction<bit<32>, _, bit<32>>(accel_avail) get_accel = {
    void apply(inout bit<32> val, out bit<32> rv) {
        if(val >= ACCEL_MAX) {
            rv = 0;
        } else {
            val = val + 1;
            rv = 1;
        }
    }
};
RegisterAction<bit<32>, _, bit<32>>(accel_avail) free_accel = {
    void apply(inout bit<32> val, out bit<32> rv) {
        rv = val;
        val = val |-| 1;
    }
};

Register<bit<32>, _> (32w1) pipe_avail;
RegisterAction<bit<32>, _, bit<32>>(pipe_avail) get_pipe = {
    void apply(inout bit<32> val, out bit<32> rv) {
        if(val == 0) {
            val = 1;
            rv = 1;
        } else {
            rv = 0;
        }
    }
};

RegisterAction<bit<32>, _, bit<32>>(pipe_avail) free_pipe = {
    void apply(inout bit<32> val, out bit<32> rv) {
        val = 0;
        rv = 0;
    }
};
