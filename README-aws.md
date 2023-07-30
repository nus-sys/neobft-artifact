This is the instruction of evaluating NeoBFT with AWS EC2 instances and virtual
private cloud service.
The evaluation can reproduce the result of the scalability benchmark, i.e., 
figure 8 in the paper.
You need to bring your own AWS credential, e.g., an IAM user with full EC2 full
access permission, to perform the evaluation.
Warning: the AWS account will be charged (estimated cost XXX).

**Prepare the AWS account.**
The evaluation is performed in Hong Kong region (`ap-east-1`).
If you want to perform in another region, modify `scripts/neo100.tf` line 13.
Notice that the evaluation result may vary due to different network conditions 
of AWS regions.

In the AWS region, set up an EC2 login key pair called `Ephemeral`.

Make sure that in the AWS region, there's no existing VPC occupying the
`10.0.0.0/16` CIDR block.

On the control machine, configure for the AWS credential e.g. with AWS CLI.

**Prepare control machine.**
The control machine must be running Ubuntu 20.04 LTS.

Install Terraform following [Install | Terraform](https://developer.hashicorp.com/terraform/downloads).
Install PyREM from pypi with `pip install pyrem`.
(Install `python3-pip` apt package before that if necessary.)

Add the following content in `$HOME/.ssh/config`
```
Host *.compute.amazonaws.com
    StrictHostKeyChecking no
    UserKnownHostsFile=/dev/null
    User ubuntu
    IdentityFile [path to "Ephemeral" key pair's PEM file]
```

**Perform evaluation.**

Build artifacts
```
$ ./build.sh
```

Create evaluation network on AWS
```
$ terraform -chdir=scripts init
$ terraform -chdir=scripts apply
```

Type "yes" when getting prompted.
The `addresses.txt` file should be generated.

Prepare for evaluation
```
$ ./neo100-perpare.sh
```

This may take tens of seconds depending on network connection to AWS region.

Evaluate each data point of the scalability benchmark for several times.
The figure is plotted by taken the best result of each data point.
```
$ ./neo100-run.sh
```

This takes around 30-40 minutes, and can be repeated as wish.

End the evaluation by destroying the evaluation network
```
$ terraform -chdir=scripts destroy
```

Type "yes" when getting prompted.

**Example output of running `./neo100-run.sh`.**
```
+ python3 scripts/neo100-run.py p256
* Evaluate Crypto p256 #Replica 4
Throughput 127405.2 op/sec 50th 555.156µs 99th 1.275945ms
Throughput 134531.1 op/sec 50th 570.463µs 99th 1.381689ms
Throughput 135938.1 op/sec 50th 564.777µs 99th 934.79µs
Throughput 137047.9 op/sec 50th 587.835µs 99th 1.914999ms
Throughput 138207.0 op/sec 50th 599.802µs 99th 2.61993ms
Throughput 138045.0 op/sec 50th 596.497µs 99th 2.581663ms
Throughput 139709.9 op/sec 50th 599.673µs 99th 2.347944ms
Throughput 140627.5 op/sec 50th 597.524µs 99th 1.981998ms
* Evaluate Crypto p256 #Replica 16
Throughput 135924.6 op/sec 50th 622.691µs 99th 2.119032ms
Throughput 140287.8 op/sec 50th 614.11µs 99th 1.6868ms
Throughput 143345.2 op/sec 50th 616.145µs 99th 1.32702ms
Throughput 142837.1 op/sec 50th 644.641µs 99th 3.038019ms
Throughput 143232.7 op/sec 50th 616.666µs 99th 1.477752ms
Throughput 143745.4 op/sec 50th 638.431µs 99th 2.432441ms
Throughput 143777.4 op/sec 50th 652.868µs 99th 3.390081ms
Throughput 143166.9 op/sec 50th 645.908µs 99th 3.228837ms
* Evaluate Crypto p256 #Replica 28
Throughput 137578.7 op/sec 50th 643.821µs 99th 1.876322ms
Throughput 138999.9 op/sec 50th 664.626µs 99th 2.6949ms
Throughput 139191.5 op/sec 50th 673.276µs 99th 2.914262ms
Throughput 139592.9 op/sec 50th 661.073µs 99th 2.5266ms
Throughput 137727.0 op/sec 50th 633.981µs 99th 2.549238ms
Throughput 139646.2 op/sec 50th 670.384µs 99th 3.199522ms
Throughput 139297.6 op/sec 50th 668.477µs 99th 3.460009ms
Throughput 139519.1 op/sec 50th 658.888µs 99th 2.931692ms
Throughput 139276.9 op/sec 50th 660.781µs 99th 2.645655ms
* Evaluate Crypto p256 #Replica 40
Throughput 138348.8 op/sec 50th 668.718µs 99th 2.544824ms
Throughput 137848.6 op/sec 50th 665.487µs 99th 2.907279ms
Throughput 137167.0 op/sec 50th 672.831µs 99th 3.009946ms
Throughput 135973.7 op/sec 50th 642.105µs 99th 1.591698ms
Throughput 138195.2 op/sec 50th 668.188µs 99th 2.374295ms
Throughput 137192.8 op/sec 50th 645.37µs 99th 1.905268ms
Throughput 137116.4 op/sec 50th 682.257µs 99th 3.009553ms
Throughput 136789.7 op/sec 50th 681.737µs 99th 3.582878ms
* Evaluate Crypto p256 #Replica 52
Throughput 136256.7 op/sec 50th 679.483µs 99th 2.530671ms
Throughput 131300.9 op/sec 50th 670.964µs 99th 2.771088ms
Throughput 133230.8 op/sec 50th 654.055µs 99th 1.553521ms
Throughput 134226.3 op/sec 50th 687.777µs 99th 2.975629ms
Throughput 131637.5 op/sec 50th 683.695µs 99th 3.883352ms
Throughput 135852.8 op/sec 50th 649.02µs 99th 1.501228ms
Throughput 136522.6 op/sec 50th 681.153µs 99th 3.036422ms
Throughput 136606.0 op/sec 50th 674.905µs 99th 2.219314ms
* Evaluate Crypto p256 #Replica 64
Throughput 133577.3 op/sec 50th 696.21µs 99th 2.987891ms
Throughput 121308.5 op/sec 50th 662.75µs 99th 2.502389ms
Throughput 125908.4 op/sec 50th 662.341µs 99th 1.675508ms
Throughput 126339.1 op/sec 50th 656.653µs 99th 1.985409ms
Throughput 126995.3 op/sec 50th 656.883µs 99th 2.050247ms
Throughput 128244.6 op/sec 50th 695.03µs 99th 2.75048ms
Throughput 129456.4 op/sec 50th 662.062µs 99th 2.95908ms
Throughput 131139.0 op/sec 50th 660.636µs 99th 1.589252ms
* Evaluate Crypto p256 #Replica 76
Throughput 128574.0 op/sec 50th 698.389µs 99th 3.076785ms
Throughput 130882.9 op/sec 50th 705.25µs 99th 2.807338ms
Throughput 131500.5 op/sec 50th 709.883µs 99th 2.907225ms
Throughput 131127.6 op/sec 50th 714.436µs 99th 2.799798ms
Throughput 130319.2 op/sec 50th 721.679µs 99th 3.080612ms
Throughput 124265.2 op/sec 50th 691.859µs 99th 3.933337ms
Throughput 123815.2 op/sec 50th 688.068µs 99th 1.877377ms
* Evaluate Crypto p256 #Replica 88
Throughput 113958.4 op/sec 50th 703.813µs 99th 2.65674ms
Throughput 118126.3 op/sec 50th 689.8µs 99th 1.99903ms
Throughput 120155.0 op/sec 50th 729.012µs 99th 3.271834ms
Throughput 120042.3 op/sec 50th 707.453µs 99th 3.294064ms
Throughput 121923.6 op/sec 50th 699.773µs 99th 1.761761ms
Throughput 120445.0 op/sec 50th 709.837µs 99th 3.322496ms
* Evaluate Crypto p256 #Replica 100
Throughput 108485.1 op/sec 50th 707.766µs 99th 3.600752ms
Throughput 113187.0 op/sec 50th 734.16µs 99th 2.47593ms
Throughput 114592.5 op/sec 50th 722.31µs 99th 2.165127ms
Throughput 113859.3 op/sec 50th 717.032µs 99th 1.979263ms
Throughput 115371.2 op/sec 50th 707.155µs 99th 1.938494ms
Throughput 113570.4 op/sec 50th 720.676µs 99th 3.83135ms
Throughput 116178.6 op/sec 50th 750.436µs 99th 2.711885ms
+ python3 scripts/neo100-run.py siphash
* Evaluate Crypto siphash #Replica 4
Throughput 135666.3 op/sec 50th 508.307µs 99th 1.28582ms
Throughput 136228.9 op/sec 50th 495.11µs 99th 1.487736ms
Throughput 138155.3 op/sec 50th 529.232µs 99th 1.8025ms
Throughput 136995.8 op/sec 50th 536.273µs 99th 2.069994ms
Throughput 137112.2 op/sec 50th 524.261µs 99th 1.622238ms
Throughput 138661.9 op/sec 50th 538.967µs 99th 2.116788ms
* Evaluate Crypto siphash #Replica 16
Throughput 29174.8 op/sec 50th 419.709µs 99th 722.769µs
Throughput 34278.4 op/sec 50th 476.578µs 99th 880.956µs
Throughput 35762.2 op/sec 50th 534.313µs 99th 2.303925ms
* Evaluate Crypto siphash #Replica 28
Throughput 4860.2 op/sec 50th 352.146µs 99th 1.432246ms
Throughput 14285.8 op/sec 50th 451.855µs 99th 2.220288ms
Throughput 16766.0 op/sec 50th 484.458µs 99th 2.11644ms
Throughput 17765.0 op/sec 50th 502.164µs 99th 2.114114ms
Throughput 18833.0 op/sec 50th 508.465µs 99th 1.426899ms
Throughput 19617.0 op/sec 50th 551.309µs 99th 2.173135ms
Throughput 19556.6 op/sec 50th 550.662µs 99th 2.562351ms
* Evaluate Crypto siphash #Replica 40
Throughput 6080.8 op/sec 50th 420.969µs 99th 2.247791ms
Throughput 13203.0 op/sec 50th 551.5µs 99th 1.997314ms
Throughput 13271.4 op/sec 50th 550.036µs 99th 2.382517ms
Throughput 13204.5 op/sec 50th 550.847µs 99th 2.607644ms
Throughput 13091.4 op/sec 50th 559.94µs 99th 2.928952ms
* Evaluate Crypto siphash #Replica 52
Throughput 1958.4 op/sec 50th 447.952µs 99th 1.654926ms
Throughput 9826.6 op/sec 50th 541.251µs 99th 914.763µs
Throughput 9739.2 op/sec 50th 543.246µs 99th 903.229µs
Throughput 9817.1 op/sec 50th 539.682µs 99th 953.667µs
Throughput 9715.1 op/sec 50th 531.203µs 99th 897.816µs
* Evaluate Crypto siphash #Replica 64
Throughput 1786.2 op/sec 50th 492.715µs 99th 1.912811ms
Throughput 1782.2 op/sec 50th 493.714µs 99th 1.885374ms
Throughput 5018.2 op/sec 50th 509.915µs 99th 2.056102ms
Throughput 6472.0 op/sec 50th 531.571µs 99th 1.502843ms
Throughput 7717.1 op/sec 50th 562.413µs 99th 1.093017ms
Throughput 8088.8 op/sec 50th 548.126µs 99th 1.094751ms
Throughput 9062.9 op/sec 50th 584.21µs 99th 923.167µs
* Evaluate Crypto siphash #Replica 76
Throughput 1667.7 op/sec 50th 531.057µs 99th 1.709131ms
Throughput 1669.2 op/sec 50th 525.454µs 99th 1.989188ms
Throughput 4708.9 op/sec 50th 556.544µs 99th 2.14467ms
Throughput 5995.3 op/sec 50th 576.427µs 99th 1.522325ms
Throughput 5949.9 op/sec 50th 579.098µs 99th 1.873598ms
Throughput 7130.9 op/sec 50th 604.974µs 99th 1.176279ms
* Evaluate Crypto siphash #Replica 88
Throughput 1604.0 op/sec 50th 564.989µs 99th 1.619228ms
Throughput 1545.7 op/sec 50th 569.701µs 99th 2.084336ms
Throughput 4306.9 op/sec 50th 603.118µs 99th 2.703243ms
Throughput 5504.3 op/sec 50th 625.425µs 99th 1.849884ms
Throughput 4283.4 op/sec 50th 599.562µs 99th 2.575882ms
Throughput 5600.1 op/sec 50th 627.173µs 99th 1.649116ms
* Evaluate Crypto siphash #Replica 100
Throughput 1396.1 op/sec 50th 612.483µs 99th 2.826208ms
Throughput 1474.0 op/sec 50th 605.553µs 99th 2.202739ms
Throughput 3940.1 op/sec 50th 659.628µs 99th 3.047105ms
Throughput 4838.8 op/sec 50th 698.136µs 99th 1.970481ms
Throughput 3896.4 op/sec 50th 655.853µs 99th 3.346768ms
Throughput 4935.1 op/sec 50th 702.117µs 99th 2.340483ms
```
