/* -*- P4_16 -*- */

#include <core.p4>
#include <tna.p4>

/*************************************************************************
 ************* C O N S T A N T S    A N D   T Y P E S  *******************
**************************************************************************/
#define FPGA_PORT 9w40
#define FPGA_DEBUG_PORT 9w24

typedef bit<48> mac_addr_t;
typedef bit<16> ether_type_t;
typedef bit<32> ipv4_addr_t;

const ether_type_t ETHERTYPE_TBFT = 0x88d5;
const ether_type_t ETHERTYPE_IPV4 = 0x0800;

#define IP_PROTOCOLS_UDP    17

/*************************************************************************
 ***********************  H E A D E R S  *********************************
 *************************************************************************/

header ethernet_h {
#ifdef PKTGEN
	// this is for a quick hack to use pkt gen
	mac_addr_t src_addr;
	mac_addr_t dst_addr;
#else
	mac_addr_t dst_addr;
	mac_addr_t src_addr;
#endif
	// mac_addr_t dst_addr;
	// mac_addr_t src_addr;
    ether_type_t    ether_type;
}

header ipv4_h {
    bit<4> version;
    bit<4> ihl;
    bit<8> diffserv;
    bit<16> total_len;
    bit<16> identification;
    bit<3> flags;
    bit<13> frag_offset;
    bit<8> ttl;
    bit<8> protocol;
    bit<16> hdr_checksum;
    ipv4_addr_t src_addr;
    ipv4_addr_t dst_addr;
}

header udp_h {
    bit<16> src_port;
    bit<16> dst_port;
    bit<16> length;
    bit<16> checksum;
}

/*************************************************************************
 **************  I N G R E S S   P R O C E S S I N G   *******************
 *************************************************************************/

    /***********************  H E A D E R S  ************************/

struct my_ingress_headers_t {
    ethernet_h   ethernet;
    ipv4_h       ipv4;
    udp_h        udp;
}

    /******  G L O B A L   I N G R E S S   M E T A D A T A  *********/

struct my_ingress_metadata_t {
}

    /***********************  P A R S E R  **************************/
parser IngressParser(packet_in        pkt,
    /* User */
    out my_ingress_headers_t          hdr,
    out my_ingress_metadata_t         meta,
    /* Intrinsic */
    out ingress_intrinsic_metadata_t  ig_intr_md)
{
    /* This is a mandatory state, required by Tofino Architecture */
     state start {
        pkt.extract(ig_intr_md);
        pkt.advance(PORT_METADATA_SIZE);
        transition parse_ethernet;
    }

    state parse_ethernet {
        pkt.extract(hdr.ethernet);
        transition select (hdr.ethernet.ether_type) {
            ETHERTYPE_IPV4 : parse_ipv4;
            default : reject;
        }
    }

    state parse_ipv4 {
        pkt.extract(hdr.ipv4);
        transition select (hdr.ipv4.protocol) {
            IP_PROTOCOLS_UDP : parse_udp;
        }
    }

    state parse_udp {
        pkt.extract(hdr.udp);
        transition accept;
    }

}

    /***************** M A T C H - A C T I O N  *********************/

control Ingress(
    /* User */
    inout my_ingress_headers_t                       hdr,
    inout my_ingress_metadata_t                      meta,
    /* Intrinsic */
    in    ingress_intrinsic_metadata_t               ig_intr_md,
    in    ingress_intrinsic_metadata_from_parser_t   ig_prsr_md,
    inout ingress_intrinsic_metadata_for_deparser_t  ig_dprsr_md,
    inout ingress_intrinsic_metadata_for_tm_t        ig_tm_md)
{

    action drop(){
        ig_dprsr_md.drop_ctl = 0x1; 
	}

    action multicast(MulticastGroupId_t mcast_grp) {
        ig_tm_md.mcast_grp_a       = mcast_grp;
        ig_tm_md.level2_exclusion_id = 0xFF;
    }

    action broadcast() {
        ig_tm_md.mcast_grp_a       = 999;
        ig_tm_md.level2_exclusion_id = ig_intr_md.ingress_port;
    }

    action l2_forward(PortId_t port) {
        ig_tm_md.ucast_egress_port=port;
    }

    table l2_forwarding_decision {
        key = {
            hdr.ethernet.dst_addr : ternary;
        }
        actions = {
            l2_forward;
            broadcast;
        }
        const entries = {
            (0xb8cef62a2f94 &&& 0xffffffffffff) : l2_forward(0);
            (0xb8cef62a45fc &&& 0xffffffffffff) : l2_forward(4);
            (0xb8cef62a3f9c &&& 0xffffffffffff) : l2_forward(8);
            (0xb8cef62a30ec &&& 0xffffffffffff) : l2_forward(12);
            (0x649d99b1688e &&& 0xffffffffffff) : l2_forward(16);
            (0x649d99b1669a &&& 0xffffffffffff) : l2_forward(20);
            (0x08c0ebb6cd5c &&& 0xffffffffffff) : l2_forward(24);
            (0x01005e000000 &&& 0xffffff000000) : l2_forward(FPGA_PORT);
            (0xffffffffffff &&& 0xffffffffffff) : broadcast();
        }
    }

    table multicast_decision {
        key = {
            hdr.ethernet.dst_addr : ternary;
        }
        actions = {
            multicast;
            NoAction;
        }
        const entries = {
            (0x01005e000000 &&& 0xffffff000000) : multicast(998);
        }
    }

#ifdef MEASURE_LATENCY
    bit<32> temp0 = 0;
    bit<32> temp1 = 0;
    bit<32> temp2 = 0;
#endif

    apply {
#ifdef FPGA_DEBUG
        if (ig_intr_md.ingress_port == FPGA_PORT) {
            ig_tm_md.ucast_egress_port = FPGA_DEBUG_PORT;
        } else if (ig_intr_md.ingress_port == FPGA_DEBUG_PORT) {
            ig_tm_md.ucast_egress_port = FPGA_PORT;
        } else {
            l2_forwarding_decision.apply();
        }
#else // FPGA_DEBUG

        if(ig_intr_md.ingress_port == FPGA_PORT) {
#ifdef MEASURE_LATENCY
            // temp0 = hdr.ethernet.src_addr[31:0];
            // temp1 = ig_prsr_md.global_tstamp[31:0];
            // temp2 = temp1 - temp0;
            // hdr.ethernet.src_addr = 16w0 ++ temp2;
            ig_tm_md.ucast_egress_port= 9w0;    // send to node1
#else
            multicast_decision.apply();
#endif
        } else {
#ifdef MEASURE_LATENCY
            if(ig_intr_md.ingress_port == 9w68) {
                hdr.ethernet.src_addr = ig_prsr_md.global_tstamp;
                ig_tm_md.ucast_egress_port= FPGA_PORT;
            }
            // ig_tm_md.ucast_egress_port= 9w0;    // send to node1
#else
            l2_forwarding_decision.apply();
#endif
        }
#ifdef MEASURE_LATENCY
        ig_tm_md.bypass_egress = 0;
#else
        ig_tm_md.bypass_egress = 1;
#endif

#endif // FPGA_DEBUG
    }
}

    /*********************  D E P A R S E R  ************************/

control IngressDeparser(packet_out pkt,
    /* User */
    inout my_ingress_headers_t                       hdr,
    in    my_ingress_metadata_t                      meta,
    /* Intrinsic */
    in    ingress_intrinsic_metadata_for_deparser_t  ig_dprsr_md)
{
    apply {
        pkt.emit(hdr);
    }
}


/*************************************************************************
 ****************  E G R E S S   P R O C E S S I N G   *******************
 *************************************************************************/

    /***********************  H E A D E R S  ************************/

struct my_egress_headers_t {
    ethernet_h   ethernet;
}

    /********  G L O B A L   E G R E S S   M E T A D A T A  *********/

struct my_egress_metadata_t {
}

    /***********************  P A R S E R  **************************/

parser EgressParser(packet_in        pkt,
    /* User */
    out my_egress_headers_t          hdr,
    out my_egress_metadata_t         meta,
    /* Intrinsic */
    out egress_intrinsic_metadata_t  eg_intr_md)
{
    /* This is a mandatory state, required by Tofino Architecture */
    state start {
        pkt.extract(eg_intr_md);
#ifdef MEASURE_LATENCY
        transition parse_ethernet;
#else
        transition accept;
#endif
    }

#ifdef MEASURE_LATENCY
    state parse_ethernet {
        pkt.extract(hdr.ethernet);
        transition accept;
    }
#endif
}

    /***************** M A T C H - A C T I O N  *********************/

control Egress(
    /* User */
    inout my_egress_headers_t                          hdr,
    inout my_egress_metadata_t                         meta,
    /* Intrinsic */
    in    egress_intrinsic_metadata_t                  eg_intr_md,
    in    egress_intrinsic_metadata_from_parser_t      eg_prsr_md,
    inout egress_intrinsic_metadata_for_deparser_t     eg_dprsr_md,
    inout egress_intrinsic_metadata_for_output_port_t  eg_oport_md)
{
#ifdef MEASURE_LATENCY
    bit<32> temp0 = 0;
    bit<32> temp1 = 0;
    bit<32> temp2 = 0;
#endif 

    apply {
#ifdef MEASURE_LATENCY
        if(eg_intr_md.egress_port == 9w0) {
            temp0 = hdr.ethernet.src_addr[31:0];
            temp1 = eg_prsr_md.global_tstamp[31:0];
            temp2 = temp1 - temp0;
            hdr.ethernet.src_addr = 16w0 ++ temp2;
        }
#endif
    }
}

    /*********************  D E P A R S E R  ************************/

control EgressDeparser(packet_out pkt,
    /* User */
    inout my_egress_headers_t                       hdr,
    in    my_egress_metadata_t                      meta,
    /* Intrinsic */
    in    egress_intrinsic_metadata_for_deparser_t  eg_dprsr_md)
{
    apply {
        pkt.emit(hdr);
    }
}


/************ F I N A L   P A C K A G E ******************************/
Pipeline(
    IngressParser(),
    Ingress(),
    IngressDeparser(),
    EgressParser(),
    Egress(),
    EgressDeparser()
) pipe;

Switch(pipe) main;
