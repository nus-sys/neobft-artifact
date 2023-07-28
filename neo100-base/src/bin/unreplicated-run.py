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

async def evaluate(client_count):
    with open('addresses.txt') as addresses:
        replica_address = None
        client_addresses = []
        for line in addresses:
            [role, public_address, address] = line.split()
            if role == 'replica':
                replica_address = replica_address or (public_address, address)
            if role == 'client' and len(client_addresses) < client_count:
                client_addresses.append(public_address)
    assert replica_address
    assert len(client_addresses) == client_count

    print('clean up', file=stderr)
    await remote_sync(replica_address[0], ['pkill', 'unreplicated'])
    await gather(*[
        remote_sync(client_address, ['pkill', 'unreplicated'])
        for client_address in client_addresses])

    print('launch replica', file=stderr)
    await remote_sync(
        replica_address[0], 
        ['tmux', 'new-session', '-d', '-s', 'unreplicated', './unreplicated-replica'])
    await sleep(1)

    print('launch clients', file=stderr)
    clients = [
        await remote(
            client_address, 
            ['./unreplicated-client', replica_address[1]], 
            stdout=PIPE, stderr=PIPE)
        for client_address in client_addresses]

    print('wait clients', end='', flush=True, file=stderr)
    for client in clients:
        await client.wait()
        print('.', end='', flush=True)
    print()

    # capture output before interrupt?
    print('interrupt replica', file=stderr)
    await remote_sync(replica_address[0], ['tmux', 'send-key', '-t', 'unreplicated', 'C-c'])

    count = 0
    output_lantecy = True
    for client in clients:
        out, err = await client.communicate()
        if client.returncode != 0:
            count = None
            print(err.decode())
        if count is None:
            continue
        [client_count, latency] = out.decode().splitlines()
        count += int(client_count)
        if output_lantecy:
            print(latency)
            output_lantecy = False
    if count is not None:
        print(count / 10)

    print('clean up', file=stderr)
    await remote_sync(replica_address[0], ['pkill', 'unreplicated'])
    await gather(*[
        remote_sync(client_address, ['pkill', 'unreplicated'])
        for client_address in client_addresses])

if __name__ == '__main__':
    from sys import argv
    from asyncio import run
    if argv[1:2] == ['test']:
        run(evaluate(200))
    else:
        for client_count in [1, 2, 5, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100]:
            print(client_count)
            run(evaluate(client_count))
