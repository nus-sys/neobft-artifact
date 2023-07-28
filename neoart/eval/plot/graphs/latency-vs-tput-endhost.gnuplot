set key top left width -3

set xlabel "Throughput (ops/sec)"
set ylabel "Latency ({/Symbol m}s)"

set xrange [0:270]
set yrange [0:750]
set xtic format "%gK"

plot \
     "latency-vs-throughput-nopaxos.dat" using ($3/1000):4 title "NOPaxos" \
     with linespoints lt 1, \
     "latency-vs-throughput-endhost.dat" using ($3/1000):4 title "NOPaxos + End-host Sequencer" \
     with linespoints lt 8, \
     "latency-vs-throughput-unreplicated.dat" using ($3/1000):4 title "Unreplicated" \
     with linespoints lt 6


