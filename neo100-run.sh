#!/bin/bash -ex

python3 scripts/neo100-run.py p256 2>verbose.txt
python3 scripts/neo100-run.py siphash 2>verbose.txt
