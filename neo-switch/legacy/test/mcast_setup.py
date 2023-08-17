bfrt.pre.node.entry(990, 990,[],[0, 68]).push()
bfrt.pre.mgid.entry(990, [990], [False],[0]).push()

print ("Multicast Groups")
bfrt.pre.mgid.dump(table=True)
print ("Multicast Nodes")
bfrt.pre.node.dump(table=True)