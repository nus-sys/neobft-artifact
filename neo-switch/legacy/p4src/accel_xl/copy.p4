action copy_sip() {
    hdr.sip.m_0 = hdr.bft.sess_num ++ hdr.bft.shard_num;
    hdr.sip.m_1 = hdr.bft.msg_num;
    hdr.sip.m_2 = hdr.bft.prev_hash;
    hdr.sip.m_3 = hdr.bft.digest;
}
