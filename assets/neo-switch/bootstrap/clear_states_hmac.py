try:
    bfrt.neo_hmac = bfrt.neo_hmac_bench
except AttributeError:
    pass

bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchIngress.counter.dump(from_hw=True)
bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchIngress.counter.clear()

bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchEgress.counter.dump(from_hw=True)
bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchEgress.counter.clear()
