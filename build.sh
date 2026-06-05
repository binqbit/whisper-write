#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

cargo build --release
mkdir -p "$SCRIPT_DIR/bin"
tmp="$(mktemp "$SCRIPT_DIR/bin/.whisper-write.XXXXXX")"
install -m 755 "$SCRIPT_DIR/target/release/whisper-write" "$tmp"
mv -f "$tmp" "$SCRIPT_DIR/bin/whisper-write"

echo Build and setup completed.
