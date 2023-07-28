set key vertical maxrows 2 width -3

set xlabel "Simulated drop rate"
set ylabel "Latency (usec)"
set logscale x 10
set xrange [0.001:1]
set yrange [0:400]
set xtic format "%g%%"
set ytic format "%g"

plot \
     "latency-vs-drop-vr.dat" using ($2*100):4 title "Paxos" \
     with linespoints lt 4 pointsize 1.7, \
     "latency-vs-drop-fastpaxos.dat" using ($2*100):4 title "Fast Paxos" \
     with linespoints lt 2, \
     "latency-vs-drop-spec.dat" using ($2*100):4 title "SpecPaxos" \
     with linespoints lt 3, \
     "latency-vs-drop-nopaxos.dat" using ($2*100):4 title "NOPaxos" \
     with linespoints lt 1
