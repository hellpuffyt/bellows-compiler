#!/usr/bin/env bash
# Times examples/bench.ash natively (-O1, -O0) and interpreted. Informational.
set -e
cd "$(dirname "$0")/.."
cargo build --release >/dev/null 2>&1
B=./target/release/bellows
T=$(mktemp -d)
$B build examples/bench.ash -O1 -o "$T/o1" >/dev/null 2>&1
$B build examples/bench.ash -O0 -o "$T/o0" >/dev/null 2>&1
t() { s=$(date +%s%N); "$@" >/dev/null; echo "$(( ($(date +%s%N)-s)/1000000 )) ms"; }
printf 'native  -O1  '; t "$T/o1"
printf 'native  -O0  '; t "$T/o0"
printf 'interp  -O1  '; t $B interp examples/bench.ash
printf 'interp  -O0  '; t $B interp examples/bench.ash -O0
$B stats examples/bench.ash
rm -rf "$T"
