import re
from sys import stdin, argv


tputs = []
for line in stdin:
    match = re.match(r"\[C\] \* interval throughput (\d+) ops/sec", line)
    if match:
        tputs.append(int(match[1]))
tputs = tputs[len(tputs) // 2:]
print(sum(tputs) / len(tputs) * int(argv[1]))