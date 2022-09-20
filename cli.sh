#!/bin/bash


# Be sure to pass CLI arguments along into cargo run
cargo build
./target/debug/btu "$@"
