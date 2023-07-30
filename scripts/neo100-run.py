#!/usr/bin/env python3
from asyncio import create_subprocess_exec, gather, sleep
from subprocess import PIPE
from sys import stderr

async def remote(address, args, stdout=None, stderr=None):
    return await create_subprocess_exec(
        'ssh', '-q', address, *args, stdout=stdout, stderr=stderr)

async def remote_sync(address, args):
    p = await remote(address, args)
    await p.wait()
    return p.returncode

# async def setup_relay(address, multicast_address):
#     code = await remote_sync(address, ['sudo', 'apt-get', 'install', '--yes', 'socat'])
#     assert code == 0
#     code = await remote_sync(
#         address, [
#             'tmux', 'new-session', '-d', '-s', 'neo'
#             'socat', 'udp4-recv:5001,ip-add-membership=239.255.1.1:ens5', f'udp4:{multicast_address}:5000'])
#     assert code == 0

# async def prepare_relay():
#     i = 1
#     tasks = []
#     with open('address.txt') as addresses:
#         for line in addresses:
#             [role, public_address, _] = line.split()
#             if role == 'relay':
#                 tasks.append(setup_relay(public_address, f'239.255.2.{i}'))
#                 i += 1
#     await gather(*tasks)


async def evaluate(f, client_count, crypto):
    replica_count = 2 * f + 1  # f replicas keep silence

    with open('addresses.txt') as addresses:
        seq_address = None
        replica_addresses, client_addresses, relay_addresses = [], [], []
        for line in addresses:
            [role, public_address, address] = line.split()
            if role == 'replica' and len(replica_addresses) < replica_count:
                replica_addresses.append((public_address, address))
            if role == 'client' and len(client_addresses) < client_count:
                client_addresses.append((public_address, None))
            if role == 'seq':
                seq_address = seq_address or (public_address, address)
            if role == 'relay':
                relay_addresses.append((public_address, address))
    assert seq_address is not None
    assert len(replica_addresses) == replica_count
    assert len(client_addresses) == client_count
    assert relay_addresses != []

    print('clean up', file=stderr)
    await gather(*[
        remote_sync(address[0], ['pkill', 'neo'])
        for address in client_addresses + replica_addresses + [seq_address] + relay_addresses])

    print('launch sequencer', file=stderr)
    await remote_sync(
        seq_address[0], [
            'tmux', 'new-session', '-d', '-s', 'neo', 
            './neo100-seq', 
                # '--multicast', '239.255.1.1', 
                '--multicast', relay_addresses[0][1], 
                # not `replica_count` here
                # the 2f + 1 replicas each need to rx 3f + 1 siphash signatures
                '--replica-count', str(3 * f + 1),
                '--crypto', crypto])

    print('launch relays', file=stderr)
    # layer 1: seq -> relay[0] -> relay[1..5]
    await remote_sync(
        relay_addresses[0][0], [
            'tmux', 'new-session', '-d', '-s', 'neo', 
            './neo100-relay', *[address[1] for address in relay_addresses[1:5]]])
    # layer 2: relay[0] -> relay[1..5] -> relay[5..21]
    await gather(*[
        remote_sync(
            relay_address[0], [
                'tmux', 'new-session', '-d', '-s', 'neo',
                './neo100-relay', *[address[1] for address in relay_addresses[5 + 4 * i : 5 + 4 * (i + 1)]]])
        for i, relay_address in enumerate(relay_addresses[1:5])
    ])
    # layer 3: relay[1..5] -> relay[5..21] -> replicas
    relay_count = len(relay_addresses) - 5
    async def launch(i, address):
        send_addresses = [
            address[1] for address in
                replica_addresses[replica_count * i // relay_count : replica_count * (i + 1) // relay_count]]
        await remote_sync(
            address, [
                'tmux', 'new-session', '-d', '-s', 'neo', './neo100-relay', *send_addresses])
    await gather(*[launch(i, address[0]) for i, address in enumerate(relay_addresses[5:])])

    # print('pending multicast group ready', file=stderr)
    # await sleep(10)

    print('launch replicas', file=stderr)
    await gather(*[remote_sync(
        replica_address[0], [
            'tmux', 'new-session', '-d', '-s', 'neo', 
            './neo100-replica', 
                '--id', str(i), 
                '--multicast', '239.255.1.1',  # not used, just placeholder 
                '-f', str(f), 
                # '--tx-count', '5',
                '--crypto', crypto])
        for i, replica_address in enumerate(replica_addresses)])
    await sleep(1)

    print('launch clients', file=stderr)
    clients = [
        await remote(
            client_address[0], [
                './neo100-client', '--seq-ip', seq_address[1], '-f', str(f)], 
            stdout=PIPE, stderr=PIPE)
            # stdout=PIPE)
        for client_address in client_addresses]

    print('wait clients', end='', flush=True, file=stderr)
    for client in clients:
        await client.wait()
        print('.', end='', flush=True, file=stderr)
    print(file=stderr)

    # capture output before interrupt?
    print('interrupt sequencer, relays and replicas', file=stderr)
    await gather(*[
        remote_sync(address[0], ['tmux', 'send-key', '-t', 'neo', 'C-c'])
        for address in replica_addresses + [seq_address] + relay_addresses])

    count = 0
    output_lantecy = True
    for client in clients:
        out, err = await client.communicate()
        if client.returncode != 0:
            count = None
            print(err.decode(), file=stderr)
        if count is None:
            break
        [client_count, latency] = out.decode().splitlines()
        count += int(client_count)
        if output_lantecy:
            # print(latency)
            output_lantecy = False
    if count is not None:
        print('Throughput', count / 10, 'op/sec', latency.strip())

    print('clean up', file=stderr)
    await gather(*[
        remote_sync(address[0], ['pkill', 'neo'])
        for address in client_addresses + replica_addresses + [seq_address]])
    return count

if __name__ == '__main__':
    from sys import argv
    from asyncio import run
    if argv[1:2] == ['test']:
        run(evaluate(0, 1, argv[2]))
    else:
        # client_count = 90
        # for replica_count in range(1, 34, 4):
        #     print(replica_count, client_count)
        #     retry = True
        #     while retry:
        #         retry = run(evaluate(replica_count, client_count, argv[1])) is None

        client_count = 90
        for replica_count in range(1, 34, 4):
            print(f'* Evaluate Crypto {argv[1]} #Replica {replica_count * 3 + 1}')
            step = 10
            for _ in range(10):
                print(replica_count, client_count, file=stderr)
                retry = run(evaluate(replica_count, client_count, argv[1])) is None
                if retry:
                    client_count -= step
                    client_count = max(client_count, 1)
                else:
                    step = max(step // 2, 1)
                    client_count += step
                    client_count = min(client_count, 100)
