#!/usr/bin/env bash
# Live-preview a book with auto-reload on file change.
# Usage: ./docs-site/serve.sh [en|es]    (default: en)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LANG="${1:-en}"

if [[ "$LANG" != "en" && "$LANG" != "es" ]]; then
  echo "Usage: $0 [en|es]" >&2
  exit 2
fi

echo "==> Serving $LANG book on http://localhost:3000"
mdbook serve "$ROOT/$LANG" --open
