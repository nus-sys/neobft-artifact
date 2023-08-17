#! /bin/bash

# setup ports
$SDE/run_bfshell.sh -f `pwd`/bootstrap/port_setup
$SDE/run_bfshell.sh -b `pwd`/bootstrap/bcast_setup.py
$SDE/run_bfshell.sh -b `pwd`/bootstrap/mcast_setup_xl.py
$SDE/run_bfshell.sh -b `pwd`/bootstrap/session_setup_pub_xl.py

# setup 
python ~/tools/run_pd_rpc.py `pwd`/bootstrap/tm_setup