#!/bin/bash
set -e

ARCH=$(uname -m)
if [ "$ARCH" = "x86_64" ]; then
    TAILWIND_BIN="tailwindcss-macos-x64"
else
    TAILWIND_BIN="tailwindcss-macos-arm64"
fi

# Re-use a cached binary if already present in the project root.
if [ ! -f "./$TAILWIND_BIN" ]; then
    echo "Downloading Tailwind CLI v4 ($TAILWIND_BIN)..."
    curl -sLO "https://github.com/tailwindlabs/tailwindcss/releases/latest/download/$TAILWIND_BIN"
    chmod +x "$TAILWIND_BIN"
    echo "Cached to ./$TAILWIND_BIN (gitignored)"
else
    echo "Using cached Tailwind CLI ($TAILWIND_BIN)"
fi

echo "Compiling CSS (Tailwind v4)..."
./$TAILWIND_BIN -i src/ui/css/input.css -o src/ui/styles.css --minify

echo "Done! CSS compiled to src/ui/styles.css ($(wc -c < src/ui/styles.css) bytes)"
