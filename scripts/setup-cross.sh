#!/bin/bash
set -e
echo "Installing cross-compilation toolchain..."
cargo install cross --git https://github.com/cross-rs/cross
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
echo "Done. Run 'make release' to build all targets."
