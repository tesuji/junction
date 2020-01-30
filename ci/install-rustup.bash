#!/usr/bin/env bash
# Install/update rustup.
#
# It is helpful to have this as a separate script due to some issues on
# Windows where immediately after `rustup self update`, rustup can fail with
# "Device or resource busy".

set -e

# Install/update rustup.
if command -v rustup > /dev/null; then
    echo "$(rustup -V)" already installed
    rustup set profile minimal
else
    curl -sSL https://sh.rustup.rs | sh -s -- -y --default-toolchain=none --profile=minimal
    echo "##[add-path]$HOME/.cargo/bin"
fi
