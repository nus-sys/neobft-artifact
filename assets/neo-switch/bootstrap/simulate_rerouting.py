import time 

start = time.time()
bfrt.pre.mgid.delete(MGID=998)
print ("Multicast Groups ==> AFTER DELETION ")
bfrt.pre.mgid.dump(table=True)


bfrt.tom_accel.tom_pipe_0.TomPipe0SwitchIngress.session.add(REGISTER_INDEX=0, f1=2)
bfrt.tom_accel.tom_pipe_0.TomPipe0SwitchIngress.session.dump(table=True, from_hw=True)

end = time.time()
bfrt.pre.mgid.entry(998, [998], [False],[0]).push()
print ("Multicast Groups ==> AFTER INSERTION")
bfrt.pre.mgid.dump(table=True)

print("Time taken:" + str(end-start))
