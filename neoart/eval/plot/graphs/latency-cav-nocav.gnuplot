set key vertical top left width -5

set xlabel "Latency ({/Symbol m}s)"
set ylabel "CDF"

set xrange [0:120]
set yrange [0:1]

plot \
     "latency-cdf-cavium.dat" using 1:3 title "With Cavium Processor" \
     with lines lt 1 lw 4, \
     "latency-cdf-nocavium.dat" using 1:3 title "Without Cavium Processor" \
     with lines lt 2 lw 4, \
