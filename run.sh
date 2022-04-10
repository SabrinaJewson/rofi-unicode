#!/bin/sh
set -eu

cargo build
cargo dev generate
sudo target/debug/dev install --profile dev
rofi -modi unicode-selector -show unicode-selector
