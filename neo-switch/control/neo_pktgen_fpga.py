from scapy.all import *

# eth = Ether(src='01:00:5e:00:00:00', dst='01:00:5e:00:00:00', type=0x0800) # 14
p = Ether(src='01:00:5e:00:00:00', dst='01:00:5e:00:00:00') / IP() / UDP(sport=12345, dport=60004) / (b'\x00' * 68) / (b'\x11' * 50) 

hexdump(p)
print(len(p))

# Write the packet to the pktgen buffer
# skip the first 6 bytes for pktgen header
# pktgen.write_pkt_buffer(64, len(p) - 6, bytes(p)[6:]) # buffer offset, buffer size, buffer data
pktgen.write_pkt_buffer(0, len(p) - 6, bytes(p)[6:]) # buffer offset, buffer size, buffer data

# enable pktgen on pipe 0's port 68 (100Gbps)
pktgen.enable(68)  # port 68

# create the app configuration
app_cfg = pktgen.AppCfg_t()
app_cfg.trigger_type = pktgen.TriggerType_t.TIMER_PERIODIC
app_cfg.timer = 900 # 1.11Mpps  # 99%  
# app_cfg.timer = 1775 # 0.56Mpps # 50%
# app_cfg.timer = 3500 # 0.28Mpps # 25%

# app_cfg.batch_count = 0 # sets no. of batches that we want to have; the batch_id field of pktgen header keeps incrementing until this value is reached
# app_cfg.pkt_count = PKTS_COUNT - 1 # sets no. of packets that we want to have in a batch; the packet_id field of pktgen header keeps incrementing until this value is reached. We are doing -1 in the above case because the numbering is starting from 0. pkt_count = 0 means 1 pkt per batch and batch_count = 0 means 1 batch per trigger
app_cfg.src_port = 68   # pipe local src port
app_cfg.buffer_offset = 0
app_cfg.length = len(p) - 6

# configure app id 1 with the app config
pktgen.cfg_app(1, app_cfg)
conn_mgr.complete_operations()

###################################################################################################

# -------------------- START PKTGEN TRAFFIC-------------- #
# pktgen.app_enable(1)
# print("PktGen Traffic Started")
# pktgen.app_disable(1)
# pktgen.show_counters(same=True)