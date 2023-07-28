set key vertical maxrows 2 width 0

set xlabel "Simulated drop rate"
set ylabel "Throughput (ops/sec)"
set logscale x 10
set xrange [0.001:1]
set yrange [0:400]
set xtic format "%g%%"
set ytic format "%gK"

plot \
     "12.dat" using ($1*100):($2/1000) title "Neo-HM" \
     with linespoints lt 4 pointsize 1.7, \
     "13.dat" using ($1*100):($2/1000) title "Neo-PK" \
     with linespoints lt 2, 
