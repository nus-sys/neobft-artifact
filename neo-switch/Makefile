all: hmac fpga

debug: hmac_debug fpga_debug

hmac:
	$(HOME)/tools/p4_build.sh ./p4src/switch/tom_hmac.p4

fpga: 
	$(HOME)/tools/p4_build.sh ./p4src/fpga/tom_fpga.p4

hmac_debug:
	$(HOME)/tools/p4_build.sh ./p4src/switch/tom_hmac.p4 -DPKTGEN -DMEASURE_LATENCY -DDEBUG

fpga_debug:
	$(HOME)/tools/p4_build.sh ./p4src/fpga/tom_fpga.p4 -DPKTGEN -DMEASURE_LATENCY 