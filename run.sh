#!/bin/sh
set -eu

ROFI_PREFIX="${ROFI_PREFIX:-}"

cd "$(dirname "$0")"
cargo build

mkdir -p "$ROFI_PREFIX"/lib/rofi
if ! cp target/debug/librofi_unicode.so "$ROFI_PREFIX"/lib/rofi/unicode.so
then
	echo Attempting to copy again as root
	sudo cp target/debug/librofi_unicode.so "$ROFI_PREFIX"/lib/rofi/unicode.so
fi

# DEBUGGER can be e.g. "gdb --args"
${DEBUGGER:-} \
	"$ROFI_PREFIX"/bin/rofi -modi unicode-selector -show unicode-selector "$@"
