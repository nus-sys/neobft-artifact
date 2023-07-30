terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 4.16"
    }
  }

  required_version = ">= 1.2.0"
}

provider "aws" {
  region = "ap-east-1"
}

data "aws_ami" "ubuntu" {
  most_recent = true

  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd/ubuntu-focal-20.04-amd64-server-*"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }

  owners = ["099720109477"] # Canonical
}

resource "aws_vpc" "neo" {
  cidr_block           = "10.0.0.0/16"
  enable_dns_hostnames = true
}

resource "aws_subnet" "neo" {
  vpc_id                  = resource.aws_vpc.neo.id
  cidr_block              = "10.0.0.0/16"
  map_public_ip_on_launch = true
}

resource "aws_internet_gateway" "neo" {
  vpc_id = resource.aws_vpc.neo.id
}

resource "aws_route_table" "neo" {
  vpc_id = resource.aws_vpc.neo.id

  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = resource.aws_internet_gateway.neo.id
  }
}

resource "aws_route_table_association" "_1" {
  route_table_id = resource.aws_route_table.neo.id
  subnet_id      = resource.aws_subnet.neo.id
}

resource "aws_security_group" "neo" {
  vpc_id = resource.aws_vpc.neo.id

  ingress {
    from_port        = 0
    to_port          = 0
    protocol         = "-1"
    cidr_blocks      = ["0.0.0.0/0"]
    ipv6_cidr_blocks = ["::/0"]
  }

  egress {
    from_port        = 0
    to_port          = 0
    protocol         = "-1"
    cidr_blocks      = ["0.0.0.0/0"]
    ipv6_cidr_blocks = ["::/0"]
  }
}

resource "aws_instance" "clients" {
  count = 100

  ami                    = data.aws_ami.ubuntu.id
  instance_type          = "t3.micro"
  subnet_id              = resource.aws_subnet.neo.id
  vpc_security_group_ids = [resource.aws_security_group.neo.id]
  key_name               = "Ephemeral"
}

resource "aws_instance" "replicas" {
  count = 69

  ami                    = data.aws_ami.ubuntu.id
  instance_type          = "m5.4xlarge"
  subnet_id              = resource.aws_subnet.neo.id
  vpc_security_group_ids = [resource.aws_security_group.neo.id]
  key_name               = "Ephemeral"
}

resource "aws_instance" "relays" {
  count = 21

  ami                    = data.aws_ami.ubuntu.id
  instance_type          = "m5.4xlarge"
  subnet_id              = resource.aws_subnet.neo.id
  vpc_security_group_ids = [resource.aws_security_group.neo.id]
  key_name               = "Ephemeral"
}

resource "aws_instance" "seq" {
  ami                    = data.aws_ami.ubuntu.id
  instance_type          = "m5.4xlarge"
  subnet_id              = resource.aws_subnet.neo.id
  vpc_security_group_ids = [resource.aws_security_group.neo.id]
  key_name               = "Ephemeral"
}

resource "local_file" "addresses" {
  content = templatefile("${path.module}/addresses.txt.tftpl", {
    clients       = zipmap(resource.aws_instance.clients[*].public_dns, resource.aws_instance.clients[*].private_ip)
    replicas      = zipmap(resource.aws_instance.replicas[*].public_dns, resource.aws_instance.replicas[*].private_ip)
    relays        = zipmap(resource.aws_instance.relays[*].public_dns, resource.aws_instance.relays[*].private_ip)
    seq-public-ip = resource.aws_instance.seq.public_dns
    seq-ip        = resource.aws_instance.seq.private_ip
  })
  filename = "${path.module}/../addresses.txt"
}
