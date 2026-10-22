#!/usr/bin/env bash
# Classify + report against the synthetic fixture (no live browser).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BIN="${BIN:-./target/debug/bpb}"
if [[ ! -x "$BIN" ]]; then
  cargo build -q
  BIN=./target/debug/bpb
fi
OUT="${OUT:-/tmp/bpb-smoke}"
rm -rf "$OUT"
mkdir -p "$OUT"
"$BIN" classify --input examples/sample-fixture.json --out "$OUT/classified.json"
"$BIN" report --input "$OUT/classified.json" --out-dir "$OUT" --redact
echo "OK: $OUT/report.json $OUT/report.md"
