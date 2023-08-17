This is the instruction of evaluating benchmark results of NeoBFT.

**Prepare the hardware.**
Connect 5 servers and 1 Xilinx AU50 FPGA to a Tofino switch.
4 of the servers should be connected into physical ports 9, 10, 41 and 42 and
running replicas, and the last server should be connected into physical port 44
and running clients.
The FPGA should be connected to the physical port 46.

Servers should run Ubuntu 20.04 LTS.
Switch should install Tofino SDE 9.7.0 and the ICA tools helper scripts.
The FPGA host machine should install XXX.

Refer to [the AWS instruction](./README-aws.md) to set up scalable benchmark's 
hardware.

**Prepare control machine.**
A control machine is used to build the artifacts and orchestrate the 
evaluations.
It could be any machine that has password-less access to the servers and has
access to the switch, including the 5 servers themselves.

The control machine should also run Ubuntu 20.04 LTS.

Install Rust toolchain according to instructions on [rustup.rs](https://rustup.rs/).

**Prepare the switch.**
Copy the `neo-switch` directory to switch, then follow the README in it on the 
switch (the *Evaluating NeoBFT* section).

**Prepare the FPGA.**
XXX

**Prepare network specification.**
Copy `spec-example.toml` to `spec.toml`.
Update the `replica` and `client` sections.
The `control-host` and `control-user` should match the SSH setup of control
machine.
The `ip` should match the IPs assigned to the NICs that connected to the switch.

**Build artifacts.**
```
$ ./build.sh
```

**Produce results of in-network micro-benchmarks (figure 4, 5 and 6).**
Follow the *Microbenchmarks* section of `neo-switch/README.md` on the switch.

**Produce results of performance benchmarks (figure 7, 9 and 10).**
Before running benchmark for `NeoBFT-HM` and `NeoBFT-BN`, start AOM-HM switch 
program following the instruction in `neo-switch/README.md`.
Before running benchmark for `NeoBFT-PK`, start AOM-PK switch program.
The other benchmarks can be running under either of the switch program.

Copy the desired part of `neo4-presets.toml` into `spec.toml`, then
```
$ ./artifact/neo4-run
```

**Produce results of the scalable benchmark (figure 8).**
Follow the *Perform Evaluation* section of `README-aws.md`.
