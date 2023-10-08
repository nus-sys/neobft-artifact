client_port = 36

bfrt.pre.node.entry(1, 1,[],[0]).push()
bfrt.pre.node.entry(2, 2,[],[4]).push()
bfrt.pre.node.entry(3, 3,[],[8]).push()
bfrt.pre.node.entry(4, 4,[],[12]).push()
# bfrt.pre.node.entry(5, 5,[],[16]).push()
# bfrt.pre.node.entry(6, 6,[],[20]).push()
bfrt.pre.node.entry(6, 6,[],[client_port]).push()

bfrt.pre.prune.mod(0, [0])
bfrt.pre.prune.mod(4, [4])
bfrt.pre.prune.mod(8, [8])
bfrt.pre.prune.mod(12, [12])
# bfrt.pre.prune.mod(16, [16])
# bfrt.pre.prune.mod(20, [20])
bfrt.pre.prune.mod(client_port, [client_port])

bfrt.pre.mgid.entry(999, [1,2,3,4,5,6], [False, False, False, False, False, False],[0,0,0,0,0,0]).push()

# print ("Multicast Groups")
# bfrt.pre.mgid.dump(table=True)
# print ("Multicast Nodes")
# bfrt.pre.node.dump(table=True)
# print ("Multicast Pruning")
# bfrt.pre.prune.dump(table=True)
