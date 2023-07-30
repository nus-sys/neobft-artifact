#!/bin/bash -ex

python3 scripts/neo100-setup.py seq relay replica
python3 scripts/neo100-update.py
