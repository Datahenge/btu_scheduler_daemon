#!/bin/bash

sudo apt install musl-tools


# VERY IMPORTANT to 'export' the environment variables.
export PKG_CONFIG_ALLOW_CROSS=1
# PKG_CONFIG_SYSROOT_DIR=/
# export PKG_CONFIG_SYSROOT_DIR=/

cargo build --release --target x86_64-unknown-linux-musl


# export OPENSSL_STATIC=true
# export OPENSSL_DIR=/opt/openssl
# OPENSSL_INCLUDE_DIR=/opt/openssl/include/openssl/
# OPENSSL_LIB_DIR=/opt/openssl/lib/

