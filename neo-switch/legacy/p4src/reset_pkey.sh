#! /bin/bash

$SDE/run_bfshell.sh -f `pwd`/bootstrap/port_reset
$SDE/run_bfshell.sh -b `pwd`/bootstrap/clear_states.py