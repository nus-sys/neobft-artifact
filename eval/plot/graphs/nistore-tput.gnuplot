set rmargin 2
set lmargin 8
set bmargin 5
set nokey

set ylabel "Max Throughput (txns/sec)"
set ylabel font ",12" offset 0
set ytic 50
set ytic format "%gK"
set yrange [0:260]

set xtic scale 0

set style fill solid 1
set boxwidth 0.6 relative
set xtics rotate by -45 offset -1,0
plot \
     "9.dat" using 0:($2/1000):3:xticlabels(1) with boxes lc variable
