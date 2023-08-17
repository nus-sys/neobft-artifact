const ether_type_t ETHERTYPE_IPV4 = 16w0x0800;
const ether_type_t ETHERTYPE_RECIRC = 16w0x8888;

const ip_protocol_t IP_PROTOCOLS_TCP = 6;
const ip_protocol_t IP_PROTOCOLS_UDP = 17;

const bit<3> MIRROR_TYPE_ACCEL = 1;

/* SipHash constants */
const bit<32> const_0 = 0x70736575;
const bit<32> const_1 = 0x6e646f6d;
const bit<32> const_2 = 0x6e657261;
const bit<32> const_3 = 0x79746573;

#define SIP_KEY_0 0x33323130
#define SIP_KEY_1 0x42413938

#define BFT_PORT 22222
#define BFT_HOLD 33333
#define BFT_COPY 44444

#define ACCEL_PORT 28
#define ACCEL_MAX 30

#define PIPE_0_RECIRC 68
#define PIPE_0_HOLDING 60
#define PIPE_0_COPY 56
#define PIPE_1_RECIRC_0 192
#define PIPE_1_RECIRC_1 196
#define PIPE_2_RECIRC 324
#define PIPE_3_RECIRC_0 448
#define PIPE_3_RECIRC_1 452