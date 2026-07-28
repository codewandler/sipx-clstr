#!/usr/bin/env bash
# Thin wrapper so the gate has one shape for every step. The checks live in check-docs.py.
set -euo pipefail
exec python3 "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/check-docs.py" "$@"
