#!/bin/bash

python3 -m venv pyvenv.d
source pyvenv.d/bin/activate
pip install --upgrade pip
pip install rq
