#! /bin/bash

# setup ports
$SDE/run_bfshell.sh -f `pwd`/bootstrap/port_setup
$SDE/run_bfshell.sh -b `pwd`/bootstrap/bcast_setup.py
$SDE/run_bfshell.sh -b `pwd`/bootstrap/mcast_setup.py