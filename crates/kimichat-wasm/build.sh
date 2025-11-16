#!/bin/bash
set -e

echo "Building KimiChat WASM module..."

# Build with wasm-pack
wasm-pack build --target web --out-dir ../../web/pkg

echo "WASM build complete! Output in web/pkg/"
