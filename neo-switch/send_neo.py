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

eth = Ether(src='00:00:00:00:00:00', dst='01:00:5e:00:00:00', type=0x88d5)
pkt = eth/ (b'\x00' * 100)
# print(pkt)
# print(pkt.show())
hexdump(pkt)
sendp(pkt, iface=iface)
