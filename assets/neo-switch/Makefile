all: hmac fpga hmac_bench fpga_bench

hmac:
	$(HOME)/tools/p4_build.sh ./p4src/switch/neo_hmac.p4

fpga: 
	$(HOME)/tools/p4_build.sh ./p4src/fpga/neo_fpga.p4

hmac_bench:
	$(HOME)/tools/p4_build.sh ./p4src/switch/neo_hmac_bench.p4 -DPKTGEN -DMEASURE_LATENCY -DDEBUG

fpga_bench:
	$(HOME)/tools/p4_build.sh ./p4src/fpga/neo_fpga_bench.p4 -DPKTGEN