#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

cargo build --release
mkdir -p "$SCRIPT_DIR/bin"
cp "$SCRIPT_DIR/target/release/whisper-write" "$SCRIPT_DIR/bin/whisper-write"

echo Build and setup completed.
