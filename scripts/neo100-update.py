#!/usr/bin/env python3
from sys import argv
from pyrem.host import LocalHost, RemoteHost
from pyrem.task import Parallel

# package = (argv[1:2] + ['dsys'])[0]
# LocalHost().run(['cargo', 'build', '--package', package, '--release'], require_success=True).start(wait=True)
# prefix = (argv[1:2] + ['unreplicated'])[0]
prefix = 'neo100'
with open('addresses.txt') as addresses:
    tasks = []
    for line in addresses:
        [role, address, _] = line.split()
        file_name = f'{prefix}-{role}'
        task = RemoteHost(address).send_file(f'artifact/{file_name}', file_name, quiet=True)
        tasks.append(task)
Parallel(tasks).start(wait=True)
