#!/bin/bash

cargo build --release
sudo ./target/release/btu-daemon

RET=$?

if [[ "$RET" -eq 1 ]]; then
    echo "BTU daemon exited with error code $RET."
fi
