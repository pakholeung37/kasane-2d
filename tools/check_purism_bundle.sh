#!/bin/sh
set -eu

compiler=$1
bundle_script=$2
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM

sh "$bundle_script" > "$work/PurismCoreBundle.h"
cat > "$work/smoke.c" <<'EOF'
#define PURISM_CORE_IMPLEMENTATION
#include "PurismCoreBundle.h"

int main(void) { return csmGetVersion() == 0; }
EOF
"$compiler" -std=c99 "$work/smoke.c" -lm -o "$work/smoke"
"$work/smoke"
