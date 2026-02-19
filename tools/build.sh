#!/bin/bash
set -e
profile=${1:-debug}
if [ "$profile" = "release" ]; then flags="--release"; else flags=""; fi
cargo build $flags --bins
mkdir -p python/exhash.data/scripts
cp target/$profile/exhash target/$profile/lnhashview python/exhash.data/scripts/
