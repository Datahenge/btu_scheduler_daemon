#!/bin/bash

sudo apt install musl-tools


# VERY IMPORTANT to 'export' the environment variables.
export PKG_CONFIG_ALLOW_CROSS=1
export OPENSSL_STATIC=true
export OPENSSL_DIR=/opt/openssl
# OPENSSL_INCLUDE_DIR=/opt/openssl/include/openssl/
# OPENSSL_LIB_DIR=/opt/openssl/lib/
cargo build --target x86_64-unknown-linux-musl
