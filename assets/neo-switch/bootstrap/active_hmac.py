try:
    bfrt.neo_hmac = bfrt.neo_hmac_bench
except AttributeError:
    pass
bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchIngress.active.mod(REGISTER_INDEX=0, f1=1)
bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchIngress.active.dump(from_hw=True)
