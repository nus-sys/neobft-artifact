set key top right

set xlabel "Throughput (ops/sec)"
set ylabel "Latency ({/Symbol m}s)"

set xrange [0:410]
set yrange [0:2200]
set xtic format "%gK"
set key horizontal width -5 over

plot \
     "1.dat" using ($2/1000):3 title "Unreplicated" with linespoints lt 1, \
     "4.dat" using ($2/1000):3 title "Neo-BN" with linespoints lt 4, \
     "6.dat" using ($2/1000):3 title "PBFT" with linespoints lt 7, \
     "2.dat" using ($2/1000):3 title "Neo-HM" with linespoints lt 2, \
     "5.dat" using ($2/1000):3 title "Zyzzyva" with linespoints lt 5, \
     "7.dat" using ($2/1000):3 title "HotStuff" with linespoints lt 8, \
     "3.dat" using ($2/1000):3 title "Neo-PK" with linespoints lt 3, \
     "8.dat" using ($2/1000):3 title "Zyzzyva-F" with linespoints lt 6, \
     "14.dat" using ($2/1000):3 title "MinBFT" with linespoints lt 9,
