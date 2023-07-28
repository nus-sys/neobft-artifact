set macro

set loadpath 'graphs/data'

red000 = "#F9B7B0"
red025 = "#F97A6D"
red050 = "#E62B17"
red075 = "#8F463F"
red100 = "#6D0D03"

blue000 = "#A9BDE6"
blue025 = "#7297E6"
blue050 = "#1D4599"
blue075 = "#2F3F60"
blue100 = "#031A49"

green000 = "#A6EBB5"
green025 = "#67EB84"
green050 = "#11AD34"
green075 = "#2F6C3D"
green100 = "#025214"

brown000 = "#F9E0B0"
brown025 = "#F9C96D"
brown050 = "#E69F17"
brown075 = "#8F743F"
brown100 = "#6D4903"

set terminal postscript color eps enhanced size 3.125,1.75 "Times-Roman" 13
set output '| ps2pdf14 -dPDFSETTINGS=/prepress -dEmbedAllFonts=true -dEPSCrop - $@'
set key top left samplen 3

set pointsize 1.1
my_line_width = 2

set style line 1 linecolor rgbcolor green025 linewidth my_line_width pt 5
set style line 2 linecolor rgbcolor blue050 linewidth my_line_width pt 7
set style line 3 linecolor rgbcolor blue050 linewidth my_line_width pt 11
set style line 4 linecolor rgbcolor blue050 linewidth my_line_width pt 13
set style line 5 linecolor rgbcolor brown050 linewidth my_line_width pt 13
set style line 6 linecolor rgbcolor brown050 linewidth my_line_width pt 5
set style line 7 linecolor rgbcolor red025 linewidth my_line_width pt 9
set style line 8 linecolor rgbcolor green050 linewidth my_line_width pt 7
set style line 9 linecolor rgbcolor red075 linewidth my_line_width pt 7
set style line 10 linecolor rgbcolor green075 linewidth my_line_width pt 11
set style line 11 linecolor rgbcolor red050 linewidth my_line_width pt 5
set style line 12 linecolor rgbcolor green100 linewidth my_line_width pt 13
set style line 13 linecolor rgbcolor red100 linewidth my_line_width pt 11
set style line 14 linecolor rgbcolor blue100 linewidth my_line_width pt 9
set style line 15 linecolor rgbcolor blue000 linewidth my_line_width pt 5
set style line 16 linecolor rgbcolor brown100 linewidth my_line_width pt 7
set style line 17 linecolor rgbcolor brown050 linewidth my_line_width pt 9

set style increment user
