#!/usr/bin/env bash
# Build both mdBook variants (EN + ES).
# Usage: ./docs-site/build.sh [--clean]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ "${1:-}" == "--clean" ]]; then
  rm -rf "$ROOT/en/book" "$ROOT/es/book"
fi

echo "==> Building English book"
mdbook build "$ROOT/en"

echo "==> Building Spanish book"
mdbook build "$ROOT/es"

echo
echo "Done."
echo "  Landing: $ROOT/index.html"
echo "  EN:      $ROOT/en/book/index.html"
echo "  ES:      $ROOT/es/book/index.html"
