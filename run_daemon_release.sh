#!/bin/bash

echo -e "Building a binary for Release ..."

cargo build --release

echo -e "Running binary..."

sudo ./target/release/btu-daemon

RET=$?

if [[ "$RET" -eq 1 ]]; then
    echo "BTU daemon exited with error code $RET."
fi
