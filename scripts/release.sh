#!/bin/bash
# TokenScavenger Release Builder
# Usage: ./scripts/release.sh [version]
# Produces static binaries for Linux x86_64 and aarch64, plus macOS Apple Silicon.

set -e

VERSION="${1:-0.1.0}"
OUTDIR="target/release-artifacts"

echo "=== TokenScavenger Release Builder v${VERSION} ==="
echo ""

# Ensure targets are installed
rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl aarch64-apple-darwin 2>/dev/null || true

# Clean build directory
rm -rf "$OUTDIR"
mkdir -p "$OUTDIR"

echo "[1/5] Building x86_64-unknown-linux-musl..."
cargo build --release --target x86_64-unknown-linux-musl
cp "target/x86_64-unknown-linux-musl/release/tokenscavenger" "$OUTDIR/tokenscavenger-${VERSION}-x86_64-linux-musl"
echo "  -> tokenscavenger-${VERSION}-x86_64-linux-musl"

echo "[2/5] Building aarch64-unknown-linux-musl..."
cargo build --release --target aarch64-unknown-linux-musl
cp "target/aarch64-unknown-linux-musl/release/tokenscavenger" "$OUTDIR/tokenscavenger-${VERSION}-aarch64-linux-musl"
echo "  -> tokenscavenger-${VERSION}-aarch64-linux-musl"

echo "[3/5] Building aarch64-apple-darwin (macOS)..."
cargo build --release --target aarch64-apple-darwin
cp "target/aarch64-apple-darwin/release/tokenscavenger" "$OUTDIR/tokenscavenger-${VERSION}-aarch64-apple-darwin"
echo "  -> tokenscavenger-${VERSION}-aarch64-apple-darwin"

echo "[4/5] Generating checksums..."
cd "$OUTDIR"
shasum -a 256 tokenscavenger-* > tokenscavenger-${VERSION}-checksums.txt
cd - > /dev/null
echo "  -> tokenscavenger-${VERSION}-checksums.txt"

# Copy config and docs
cp tokenscavenger.toml "$OUTDIR/tokenscavenger-${VERSION}-example.toml" 2>/dev/null || echo "Warning: tokenscavenger.toml not found"

echo "[5/5] Done!"
echo ""
echo "Artifacts in $OUTDIR/:"
ls -la "$OUTDIR/"
echo ""
echo "Checksums:"
cat "$OUTDIR/tokenscavenger-${VERSION}-checksums.txt"
