#!/bin/bash

cargo build
sudo ./target/debug/btu-daemon

RET=$?

if [[ "$RET" -eq 1 ]]; then
    echo "BTU daemon exited with error code $RET."
fi
