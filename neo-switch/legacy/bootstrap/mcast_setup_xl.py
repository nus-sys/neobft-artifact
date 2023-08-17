# Multicast to REPLICAs and COPY port
bfrt.pre.node.entry(888, 888,[],[0, 4, 8, 12, 56]).push()
bfrt.pre.mgid.entry(888, [888], [False],[0]).push()

# Multicast to REPLICAs, SmartNIC, and COPY port
bfrt.pre.node.entry(887, 887,[],[0, 4, 8, 12, 28, 56]).push()
bfrt.pre.mgid.entry(887, [887], [False],[0]).push()

print ("Multicast Groups")
bfrt.pre.mgid.dump(table=True)
print ("Multicast Nodes")
bfrt.pre.node.dump(table=True)

# Copied from BA Labs
# bfrt.pre.node.entry(multicast_node_id=1, multicast_rid=5,
                    # multicast_lag_id=[], dev_port=[1, 3, 8]).push()

# Create a Multicast Node with RID=10 and ports [2, 3, 7, 8]
# bfrt.pre.node.entry(2, 10, [], [2, 3, 7, 8]).push()

# # Create a Multicast Node with RID=20 and ports [5, 8]
# bfrt.pre.node.entry(3, 20, [], [5, 8]).push()

# Create a Multicast Group 1 with the nodes [1, 2, 3], no exclusion
# bfrt.pre.mgid.entry(mgid=1,
                    # multicast_node_id=[1, 2, 3],
                    # multicast_node_l1_xid_valid=[False, False, False],
                    # multicast_node_l1_xid=[0, 0, 0]).push()

