// action copy_sip00() {
//     hdr.sip00.m_0 = 8w0 ++ hdr.bft.sess_num ++ 8w0 ++ hdr.bft.shard_num;
//     hdr.sip00.m_1 = hdr.bft.msg_num;
//     hdr.sip00.m_2 = hdr.s_digest.d_0;
//     hdr.sip00.m_3 = hdr.s_digest.d_1;
// }

action copy_sip00() {
    // hdr.sip00.m_0 = hdr.bft.sess_num ++ hdr.bft.shard_num;
    // hdr.sip00.m_1 = hdr.bft.msg_num;
    // hdr.sip00.m_2 = 0;
    // hdr.sip00.m_3 = hdr.bft.digest;
    hdr.sip00.m_0 = hdr.bft.sess_num ++ hdr.bft.shard_num ++ 16w0;
    hdr.sip00.m_1 = hdr.msg_num.count;
    hdr.sip00.m_2 = hdr.bft.digest0;
    hdr.sip00.m_3 = hdr.bft.digest1;
}

action copy_sip01() {
    // hdr.sip01.m_0 = hdr.bft.pad0 ++ hdr.bft.sess_num ++ hdr.bft.pad1 ++ hdr.bft.shard_num;
    // hdr.sip01.m_1 = hdr.bft.msg_num;
    // hdr.sip01.m_2 = hdr.digest.d_0;
    // hdr.sip01.m_3 = hdr.digest.d_1;
    hdr.sip01.m_0 = hdr.sip00.m_0;
    hdr.sip01.m_1 = hdr.sip00.m_1;
    hdr.sip01.m_2 = hdr.sip00.m_2;
    hdr.sip01.m_3 = hdr.sip00.m_3;
}

action copy_sip10() {
    // hdr.sip10.m_0 = hdr.bft.pad0 ++ hdr.bft.sess_num ++ hdr.bft.pad1 ++ hdr.bft.shard_num;
    // hdr.sip10.m_1 = hdr.bft.msg_num;
    // hdr.sip10.m_2 = hdr.digest.d_0;
    // hdr.sip10.m_3 = hdr.digest.d_1;
    hdr.sip10.m_0 = hdr.sip01.m_0;
    hdr.sip10.m_1 = hdr.sip01.m_1;
    hdr.sip10.m_2 = hdr.sip01.m_2;
    hdr.sip10.m_3 = hdr.sip01.m_3;
}

action copy_sip11() {
    // hdr.sip11.m_0 = hdr.bft.pad0 ++ hdr.bft.sess_num ++ hdr.bft.pad1 ++ hdr.bft.shard_num;
    // hdr.sip11.m_1 = hdr.bft.msg_num;
    // hdr.sip11.m_2 = hdr.digest.d_0;
    // hdr.sip11.m_3 = hdr.digest.d_1;
    hdr.sip11.m_0 = hdr.sip10.m_0;
    hdr.sip11.m_1 = hdr.sip10.m_1;
    hdr.sip11.m_2 = hdr.sip10.m_2;
    hdr.sip11.m_3 = hdr.sip10.m_3;
}

// action copy_s_digest() {
//     hdr.s_digest.d_0 = hdr.digest.d_0;
//     hdr.s_digest.d_1 = hdr.digest.d_1;
// }

action init_sip_hash(bit<8> shard_num) {
    hdr.bft.shard_num = shard_num;
    hdr.sip00.setValid();
    hdr.sip01.setValid();
    // hdr.sip00_meta.setValid();
    // hdr.sip01_meta.setValid();
    hdr.sip10.setValid();
    hdr.sip11.setValid();
    // hdr.sip10_meta.setValid();
    // hdr.sip11_meta.setValid();
}

table do_init_sip_hash {
    key = {
        hdr.bft.pad0 : exact;
        hdr.bft.pad1 : exact;
    }
    actions = {
        init_sip_hash;
        NoAction;
    }
    const entries = {
        (0, 0) : init_sip_hash(8w0);
        (1, 0) : init_sip_hash(8w1);
        (2, 0) : init_sip_hash(8w2);
        (3, 0) : init_sip_hash(8w3);
    }
    default_action = NoAction();
    size = 512;
}