#!/usr/bin/env bash
# Install/update rust.

set -e
if [[ -z "$1" ]]; then
    echo "First parameter must be toolchain to install."
    exit 1
fi
TOOLCHAIN="$1"

rustup component remove --toolchain="$TOOLCHAIN" rust-docs || echo "already removed"
rustup default "$TOOLCHAIN"
rustup update --no-self-update "$TOOLCHAIN"
if [[ -n $2 ]]; then
    rustup target add "$2"
fi
rustup -V
rustc -Vv
cargo -V
