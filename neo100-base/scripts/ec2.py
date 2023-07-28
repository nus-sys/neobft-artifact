#!/usr/bin/env python3
import sys
import botocore
import boto3


profile = 'default'
region_name = 'ap-east-1'
subnet = 'subnet-008ff4a08e63a0794'  # 172.31.0.0/24
# subnet = 'subnet-099a59c8b291beba1'  # 172.31.16.0/24
image_id = 'ami-0bc44b8dc7cae9c34'  # ubuntu 22.04

# profile = 'prof'
# region_name = 'ap-southeast-1'
# subnet = 'subnet-7413812d'  # 172.31.0.0/24
# image_id = 'ami-082b1f4237bd816a1'  # ubuntu 22.04

def params_replica(i):
    assert i < 254
    ip = f'172.31.1.{i + 1}'
    return {
        'SubnetId': subnet,
        'PrivateIpAddress': ip,
        'ImageId': image_id,
        'InstanceType': 'm5.4xlarge',
        'KeyName': 'Ephemeral',
    }


def params_client(i):
    assert i < 1024
    ip = f'172.31.{2 + i // 254}.{1 + i % 254}'
    return {
        'SubnetId': subnet,
        'PrivateIpAddress': ip,
        'ImageId': image_id,
        'InstanceType': 't3.micro',
        'KeyName': 'Ephemeral',
    }


def params_seq(i):
    assert i == 0
    ip = '172.31.0.4'
    return {
        'SubnetId': subnet,
        'PrivateIpAddress': ip,
        'ImageId': image_id,
        'InstanceType': 'm5.4xlarge',
        'KeyName': 'Ephemeral',        
    }


def params_relay(i):
    assert i < 128
    ip = f'172.31.14.{1 + i}'
    return {
        'SubnetId': subnet,
        'PrivateIpAddress': ip,
        'ImageId': image_id,
        'InstanceType': 'm5.4xlarge',
        'KeyName': 'Ephemeral',
    }


params = {
    'seq': params_seq,
    'replica': params_replica,
    'client': params_client,
    'relay': params_relay,
}
boto3.setup_default_session(profile_name=profile)
ec2 = boto3.resource('ec2', region_name=region_name)


def launch(args, dry):
    instances = []
    for arg in args:
        [role, count] = arg.split('=')
        for i in range(int(count)):
            try:
                instance = ec2.create_instances(
                    **params[role](i), 
                    MinCount=1, MaxCount=1, 
                    TagSpecifications=[{
                        'ResourceType': 'instance', 
                        'Tags': [{'Key': 'dsys-role', 'Value': role}]}],
                    DryRun=dry,
                )[0]
                instances.append((role, instance))
                # addresses += f'{role:12}{instance.private_ip_address}\n'

            except botocore.exceptions.ClientError as err:
                if err.response['Error']['Code'] != 'DryRunOperation':
                    raise
    return instances


def terminate():
    instances = list(ec2.instances.filter(Filters=[
        {'Name': 'instance-state-name', 'Values': ['running']},  # other states?
        {'Name': 'tag:dsys-role', 'Values': ['*']}]))
    for instance in instances:
        instance.terminate()

    for instance in instances:
        instance.wait_until_terminated()
        print('.', end='', flush=True)
    print()
    print('terminated')


if sys.argv[1:2] == ['launch']:
    # dry run is not very useful because we launch instances one by one
    # launch(sys.argv[2:], True)
    # print('Dry run finish')

    try:
        instances = launch(sys.argv[2:], dry=False)
    except:
        terminate()
        raise

    addresses = ''
    print('requested')
    for role, instance in instances:
        instance.wait_until_running()
        instance.reload()
        addresses += f'{role:12}{instance.public_ip_address:20}{instance.private_ip_address}\n'
        print('.', end='', flush=True)
    print()
    with open('addresses.txt', 'w') as addresses_file:
        addresses_file.write(addresses)
elif sys.argv[1:2] == ['terminate']:
    terminate()
    with open('addresses.txt', 'w') as addresses_file:
        pass  # clear it
else:
    print(f'Usage: {sys.argv[0]} launch|terminate')
