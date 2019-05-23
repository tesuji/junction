#!/bin/sh

if [ -z "$1" ]; then
  toolchain=stable
else
  toolchain=$1
fi

set -ex
curl -sSLf -o rustup-init.exe 'https://win.rustup.rs/x86_64'
./rustup-init.exe -y --default-host=x86_64-pc-windows-msvc --default-toolchain="${toolchain}"
rm ./rustup-init.exe
