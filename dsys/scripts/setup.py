#!/usr/bin/env python3
from asyncio import create_subprocess_exec
from subprocess import DEVNULL

params = {
    'replica': {'interface': 'ens5', '#core': 16},
    'seq': {'interface': 'ens5', '#core': 16},
    'relay': {'interface': 'ens5', '#core': 16},
}


async def remote(address, command):
    p = await create_subprocess_exec('ssh', '-q', address, command, stdout=DEVNULL)
    await p.wait()
    assert p.returncode == 0


async def setup_remote(address, param):
    await remote(
        address,
        f"for i in $(seq {param['#core'] // 2} {param['#core'] - 1}); do "
            "echo 0 | sudo tee /sys/devices/system/cpu/cpu$i/online; done && "
        f"sudo ethtool -L {param['interface']} combined 1 && "
        f"sudo ethtool -G {param['interface']} rx 16384 && "  #
        "sudo service irqbalance stop && "
        f"IRQBALANCE_BANNED_CPULIST=0-{param['#core'] // 2 - 2} sudo -E irqbalance --oneshot && "
        # for working with AWS VPC's multicast
        f"sudo sysctl net.ipv4.conf.{param['interface']}.force_igmp_version=2")


if __name__ == '__main__':
    from asyncio import run, gather
    from sys import argv

    roles = argv[1:]
    assert all(role in params for role in roles)
    tasks = []
    with open('addresses.txt') as addresses:
        for line in addresses:
            [role, address, _] = line.split()
            if role in roles:
                tasks.append(setup_remote(address, params[role]))

    async def main():
        await gather(*tasks)

    run(main())
