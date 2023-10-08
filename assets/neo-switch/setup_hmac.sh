#! /bin/bash

# setup ports
$SDE/run_bfshell.sh -f `pwd`/bootstrap/port_setup
$SDE/run_bfshell.sh -b `pwd`/bootstrap/bcast_setup.py
$SDE/run_bfshell.sh -b `pwd`/bootstrap/mcast_setup.py
$SDE/run_bfshell.sh -b `pwd`/bootstrap/active_hmac.py
$SDE/run_bfshell.sh -b `pwd`/bootstrap/session_setup_hmac.py