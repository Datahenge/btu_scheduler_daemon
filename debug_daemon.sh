#!/bin/bash

export RUST_BACKTRACE=1
sudo RUST_BACKTRACE=1 cargo run

RET=$?

if [[ "$RET" -eq 1 ]]; then
    echo "BTU daemon exited with error code $RET."
fi
