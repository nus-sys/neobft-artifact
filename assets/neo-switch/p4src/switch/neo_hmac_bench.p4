#if __TARGET_TOFINO__ == 2
#include <t2na.p4>
#else
#include <tna.p4>
#endif

#include "type.p4"
#include "constants.p4"
#include "headers.p4"

#include "tom_pipe0.p4"
#include "tom_pipe1.p4"

// Packet flow:
// ingress tom_pipe0 -> egress tom_pipe1 -> ingress tom_pipe1 -> egress tom_pipe0

Pipeline(TomPipe0SwitchIngressParser(),
         TomPipe0SwitchIngress(),
         TomPipe0SwitchIngressDeparser(),
         TomPipe0SwitchEgressParser(),
         TomPipe0SwitchEgress(),
         TomPipe0SwitchEgressDeparser()) tom_pipe_0;

Pipeline(TomPipe1SwitchIngressParser(),
         TomPipe1SwitchIngress(),
         TomPipe1SwitchIngressDeparser(),
         TomPipe1SwitchEgressParser(),
         TomPipe1SwitchEgress(),
         TomPipe1SwitchEgressDeparser()) tom_pipe_1;

Switch(tom_pipe_0, tom_pipe_1) main;