#!/bin/bash
cargo build --release
cp ./target/release/dpc ~/.local/bin
