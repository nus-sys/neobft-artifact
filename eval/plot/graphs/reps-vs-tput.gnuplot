set key vertical maxrows 2 width 0

set xlabel "Number of Replicas"
set ylabel "Throughput (ops/sec)"
set yrange [0:200]
set ytic format "%gK"
set xtics 0,20,100


plot \
     "10.dat" using ($1*3+1):($2/1000) title "Neo-HM" \
     with linespoints lt 15, \
     "11.dat" using ($1*3+1):($2/1000) title "Neo-PK" \
     with linespoints lt 2, 
     