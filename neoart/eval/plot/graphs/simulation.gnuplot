set key vertical top left width -6.2

set xlabel "Latency ({/Symbol m}s)"
set ylabel "CDF"

set xrange [0:20]
set yrange [0:1]

plot \
     "simulation-ipmulticast.dat" using 1:3 title "IP Multicast" \
     with lines lt 1 lw 4, \
     "simulation-serialization.dat" using 1:3 title "Network Serialization" \
     with lines lt 2 lw 4, \
