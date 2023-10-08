client_port = 36

ports = (0, 4, 8, 12, client_port)
for port in ports:
    bfrt.pre.node.entry(port, port,[],[port]).push()
    bfrt.pre.prune.mod(port, [port])

bfrt.pre.mgid.entry(999, list(ports), [False for _ in ports],[0 for _ in ports]).push()

# print ("Multicast Groups")
# bfrt.pre.mgid.dump(table=True)
# print ("Multicast Nodes")
# bfrt.pre.node.dump(table=True)
# print ("Multicast Pruning")
# bfrt.pre.prune.dump(table=True)
