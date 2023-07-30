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

**Example output of running `./neo100-run.sh`.**
XXX
