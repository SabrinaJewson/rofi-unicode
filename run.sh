#!/bin/sh
set -eu

cd "$(dirname "$0")"
cargo build
cargo dev generate
sudo target/debug/dev install --profile dev
rofi -modi unicode-selector -show unicode-selector
