set size 1,0.8
set nokey

set xlabel "Time (s)"
set ylabel "Throughput (ops/sec)"
set xrange [-0.38:1.55]
set yrange [0:300]
set xtic .25
set ytic format "%gK"

set arrow from 0,graph(0,0) to 0,graph(1,1) nohead ls 2 lc rgbcolor "#777777"

plot \
     "switch-failover-nopaxos.dat" using ($1/1000 - 6.81):($2/10) with points ps 0.7
