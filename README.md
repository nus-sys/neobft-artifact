This is the instruction of evaluating benchmark results of NeoBFT.

**Prepare the hardware.**
The following setup can be used to evaluate all results except the one from 
scalability benchmark.
For that refer to [the AWS instruction](./README-aws.md).

Connect 5 servers and 1 Xilinx AU50 FPGA to a Tofino switch.
4 of the servers should be connected into physical ports 9, 10, 41 and 42 and
running replicas, and the last server should be connected into physical port 44
and running clients.
The FPGA should be connected to the physical port 46.

Servers should run Ubuntu 20.04 LTS.
Switch should install Tofino SDE 9.7.0.
The FPGA host machine should install XXX.

**Prepare control machine.**
A control machine is used to build the artifacts and orchestrate the 
evaluations.
It could be any machine that has password-less access to the servers and has
access to the switch, including the 5 servers themselves.

The control machine should also run Ubuntu 20.04 LTS.

Install Rust toolchain according to instructions on [rustup.rs](https://rustup.rs/).

**Prepare the switch.**
XXX (Modify code to match MAC addresses, dev ports, NIC spec, etc.)

```
$ bash compile_hmac.sh
$ bash compile_pkey.sh
```

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

**Produce results of in-network micro-benchmarks.**
XXX

**Produce results of performance benchmarks (figure 7, 9 and 10).**
Modify the `task` section of `spec.toml`, then
```
$ ./artifact/neo4-run
```

Repeat with all desired configuration of `task` section.

* Throughput-latency benchmark (figure 7):
  ```toml
  app = "null"
  f = 1
  assume-byz = false
  batch-size = 10
  ```
  * Unreplicated: `mode = "ur"`.
    Set `num-client` to 1, 2, 5, 10, 15, 20, 25, 50, 100, 1000.
  * Neo: `mode = "neo"`, and configure `multicast` section (see below).
    * HMAC: set `num-client` to 1, 5, 10, 25, 50, 75, 100, 1000.
    * Public key: set `num-client` to 1, 2, 5, 10, 25, 35, 50, 60, 75, 100, 500
    * HMAC in Byzantine network: set `num-client`/`batch-size` to 1/1, 2/1, 5/1, 
      10/2, 15/3, 25/5, 50/10, 75/20, 100/20, 125/20, 150/30, 175/40, 200/40, 
      500/40.
  * Zyzzyva: `mode = "zyzzyva"`.
    Set `num-client` to 1, 2, 5, 10, 30, 40, 50, 60, 75, 100, 150, 180, 200, 
    220, 250, 300.
  * Zyzzyva with Byzantine fault: `mode = "zyzzyva"` and modify `assume-byz` to
    `true`.
    Set `num-client` to 1, 2, 5, 10, 25, 50, 75, 100, 200.
  * PBFT: `mode = "pbft"`.
    Set `num-client` to 1, 5, 10, 25, 50, 75, 100, 250, 500.
  * HotStuff: `mode = "hotstuff"`.
    Set `num-client` to 1, 10, 40, 100, 150, 200, 400.
  * MinBFT: `mode = "minbft"`.
    Set `num-client` to 1, 2, 5, 10, 15, 20, 25, 50, 100, 500.
* Simulated drops benchmark (figure 9):
  ```toml
  mode = "neo"
  app = "null"
  f = 1
  assume-byz = true
  ```
  Set `drop-rate` to 0.00001, 0.00005, 0.0001, 0.0005, 0.001, 0.005, 0.01.
* Replicated key-value store (figure 10):
  ```toml
  app = "ycsb"
  f = 1
  assume-byz = false
  batch-size = 10
  ```
  * Unreplicated: `mode = "ur"` and `num-client = 40`.
  * Neo: `mode = "neo"`.
    * HMAC: `num-client = 80`.
    * Public key: `num-client = 40`.
    * HMAC in Byzantine network: `num-client = 700`.
    * Zyzzyva: `mode = "zyzzyva"` and `num-client = 100`.
    * Zyzzyva with Byzantine fault: `mode = "zyzzyva"`, `num-client = 100` and
      modify `assume-byz` to `true`.
    * PBFT: `mode = "pbft` and `num-client = 300`.
    * HotStuff: `mode = "hotstuff` and `num-client = 500`.
    * MinBFT: `mode = "minbft"` and `num-client = 100`.

Note: set network primitive variant for NeoBFT.
In `multicast` section, set `variant = "halfsiphash"` for HMAC variant, and set
`variant = "p256"` for public key variant.
