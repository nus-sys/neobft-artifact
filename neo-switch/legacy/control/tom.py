import sys
import os
import argparse

sde_install = os.environ['SDE_INSTALL']
sys.path.append('%s/lib/python2.7/site-packages/tofino'%(sde_install))
sys.path.append('%s/lib/python2.7/site-packages/p4testutils'%(sde_install))
sys.path.append('%s/lib/python2.7/site-packages'%(sde_install))

import grpc
import time
from pprint import pprint
import bfrt_grpc.client as gc
import bfrt_grpc.bfruntime_pb2 as bfruntime_pb2

def connect():
    # Connect to BfRt Server
    interface = gc.ClientInterface(grpc_addr='localhost:50052', client_id=0, device_id=0)
    target = gc.Target(device_id=0, pipe_id=0xFFFF)
    # print('Connected to BfRt Server!')

    # Get the information about the running program
    bfrt_info = interface.bfrt_info_get()
    # print('The target is running the', bfrt_info.p4_name_get())

    # Establish that you are working with this program
    interface.bind_pipeline_config(bfrt_info.p4_name_get())
    return interface, target, bfrt_info

def disable(connection):
    interface = connection[0]
    target = connection[1]
    bfrt_info = connection[2]

    active_reg = bfrt_info.table_get('pipe.SwitchIngress.active')
    key = [active_reg.make_key([gc.KeyTuple('$REGISTER_INDEX', 0)])]
    data = [active_reg.make_data([gc.DataTuple('SwitchIngress.active.f1', 0)])]
    active_reg.entry_mod(target, key, data)
    print('xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx')
    print('OPERATION: Switching is disabled! :(')
    print('xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx')

def enable(connection):
    interface = connection[0]
    target = connection[1]
    bfrt_info = connection[2]

    active_reg = bfrt_info.table_get('pipe.SwitchIngress.active')
    key = [active_reg.make_key([gc.KeyTuple('$REGISTER_INDEX', 0)])]
    data = [active_reg.make_data([gc.DataTuple('SwitchIngress.active.f1', 1)])]
    active_reg.entry_mod(target, key, data)
    print('xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx')
    print('OPERATION: Switching is enabled! :D')
    print('xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx')


def configure(connection, sequence, sess_num):
    interface = connection[0]
    target = connection[1]
    bfrt_info = connection[2]

    sess_reg = bfrt_info.table_get('pipe.SwitchIngress.session')
    key = [sess_reg.make_key([gc.KeyTuple('$REGISTER_INDEX', 0)])]
    data = [sess_reg.make_data([gc.DataTuple('SwitchIngress.session.f1', sess_num)])]
    sess_reg.entry_mod(target, key, data)

    counter_reg = bfrt_info.table_get('pipe.SwitchIngress.counter')
    key = [counter_reg.make_key([gc.KeyTuple('$REGISTER_INDEX', 0)])]
    data = [counter_reg.make_data([gc.DataTuple('SwitchIngress.counter.f1', sequence)])]
    counter_reg.entry_mod(target, key, data)
    print('xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx')
    print('OPERATION: Updated session number to ' + str(sess_num) + ' and message sequence number to ' + str(sequence))
    print('xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx')

def main():
    parser1 = argparse.ArgumentParser()
    group = parser1.add_mutually_exclusive_group()
    group.add_argument(
        '--disable', default=False, action='store_true',
        help='disable switch forwarding'
    )
    group.add_argument(
        '--enable', default=False, action='store_true',
        help='enable switch forwarding'
    )

    parser2 = argparse.ArgumentParser()
    subparsers = parser2.add_subparsers()
    subparser1 = subparsers.add_parser('config')
    subparser1.add_argument(
        '--sequence', type=int, default=1, required=True,
        help='specify starting message sequence number'
    )
    subparser1.add_argument(
        '--session', type=int, default=0, required=True,
        help='specify current session number'
    )

    args, extras = parser1.parse_known_args()
    to_disable = args.disable
    to_enable = args.enable

    if to_disable or to_enable:
        pprint(args)
        if len(extras) > 0:
            print('PARSER: Remaining arguments are omitted.')
    else:
        if len(extras) > 0 and extras[0] in ['config']:
            args = parser2.parse_args(extras, namespace=args)
            pprint(args)

    sequence = args.sequence if 'sequence' in args else None
    sess_num = args.session if 'session' in args else None
    
    if to_disable:
        disable(connect())
        return
    if to_enable:
        enable(connect())
        return
    if not sequence == None and not sess_num == None:
        configure(connect(), sequence, sess_num)
        return

    print('Nothing was done. :)')

if __name__ == '__main__':
    main()
