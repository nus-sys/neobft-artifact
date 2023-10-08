import time 

try:
    bfrt.neo_hmac = bfrt.neo_hmac_bench
except AttributeError:
    pass

# disable switch
bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchIngress.active.mod(REGISTER_INDEX=0, f1=0)
start = time.time()

time.sleep(0.1)

# update session number
# reset message number
# enable switch
bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchIngress.session.mod(REGISTER_INDEX=0, f1=2)
bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchIngress.counter.mod(REGISTER_INDEX=0, f1=0)
bfrt.neo_hmac.tom_pipe_0.TomPipe0SwitchIngress.active.mod(REGISTER_INDEX=0, f1=1)
end = time.time()

print("Time taken:" + str(end-start))
