const ether_type_t ETHERTYPE_IPV4 = 16w0x0800;
const ether_type_t ETHERTYPE_RECIRC = 16w0x8888;

const ip_protocol_t IP_PROTOCOLS_TCP = 6;
const ip_protocol_t IP_PROTOCOLS_UDP = 17;

/* SipHash constants */
const bit<32> const_0 = 0x70736575;
const bit<32> const_1 = 0x6e646f6d;
const bit<32> const_2 = 0x6e657261;
const bit<32> const_3 = 0x79746573;

#define BFT_PORT 60004