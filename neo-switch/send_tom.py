#!/usr/bin/python3

from scapy.all import *
from pprint import pprint
import argparse

parser = argparse.ArgumentParser()
parser.add_argument(
    '--iface',
    required=True,
    help='iface'
)

args = parser.parse_args()
pprint(args)

iface = args.iface


class BFT(Packet):
    name = "BFT"
    fields_desc = [
        BitField("legacy", 0, 32),
        # 32 bytes from here
        BitField("sess_num", 0, 16),
        BitField("shard_num", 0, 16),
        BitField("msg_num", 0, 32),
        BitField("digest", 0, 192)
    ]

class BFT2(Packet):
    name = "BFT"
    fields_desc = [
        BitField("legacy", 0, 32),
        # 16 bytes from here
        BitField("sess_num", 0, 16),
        BitField("shard_num", 0, 16),
        BitField("msg_num", 0, 32),
        BitField("hash", 0, 32),
        BitField("digest", 0, 32),
        BitField("hmac", 0, 128)
    ]

def gen_digest():
    b = b'\x00' * 4 + b'\x01' * 4 + b'\x02' * 4 + b'\x03' * 4 + b'\x04' * 4 + b'\x05' * 4
    assert len(b) == 24
    return int.from_bytes(b, 'big')

def gen_digest2():
    b = b'\x11' * 4 + b'\x00' * 16
    assert len(b) == 20
    return int.from_bytes(b, 'big')

eth = Ether(src='b8:ce:f6:04:6b:d0', dst='b8:ce:f6:04:6c:05')
ip = IP(src='10.10.10.1', dst='10.10.10.2', ihl=5, len=64, frag=0, flags=2, ttl=64)
udp = UDP(sport=12345, dport=22222, len=44, chksum=0)
# bft = BFT(legacy=0, sess_num=0, shard_num=0, msg_num=0, digest=gen_digest())
bft = BFT2(legacy=0, sess_num=0, shard_num=0, msg_num=0, hash=0, digest=gen_digest2())

# Sess0, Shard0, Seq0 - 9cdefd49
# Sess0, Shard1, Seq0 - ce43577f
# Sess0, Shard0, Seq1 - 5edef8db
# Sess0, Shard1, Seq1 - 90439a4a

pkt = eth/ip/udp/bft
# print(pkt)
# print(pkt.show())
sendp(pkt, iface=iface)
