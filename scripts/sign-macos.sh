#!/bin/sh
# Signs a macOS binary ad hoc with the hardened runtime and verifies it.
#
#   scripts/sign-macos.sh <binary>               sign, then verify
#   scripts/sign-macos.sh --check-only <binary>  verify only
#
# Verification passes only if `codesign --display --verbose=2` prints a
# `flags=` line naming both `adhoc` and `runtime`, e.g.
# `flags=0x10002(adhoc,runtime)`. agent-mail refuses the Keychain without
# the runtime flag (ADR-0002).
set -eu

usage() {
    echo "usage: $0 [--check-only] <binary>" >&2
    exit 2
}

check_only=0
if [ "${1:-}" = "--check-only" ]; then
    check_only=1
    shift
fi
[ $# -eq 1 ] || usage
binary=$1
[ -f "$binary" ] || { echo "sign-macos.sh: no such file: $binary" >&2; exit 1; }

if [ "$check_only" -eq 0 ]; then
    codesign --sign - --options runtime --force "$binary"
fi

details=$(codesign --display --verbose=2 "$binary" 2>&1) || {
    echo "sign-macos.sh: $binary is not signed" >&2
    exit 1
}
printf '%s\n' "$details"
# The flag names, e.g. `adhoc,runtime`, from the CodeDirectory flags= line.
flags=$(printf '%s\n' "$details" |
    sed -n 's/^CodeDirectory .*flags=0x[0-9a-fA-F]*(\([^)]*\)).*/\1/p' | head -n 1)
has_flag() {
    case ",$flags," in
        *",$1,"*) return 0 ;;
        *) return 1 ;;
    esac
}
has_flag adhoc || {
    echo "sign-macos.sh: $binary is not signed ad hoc (flags: ${flags:-none})" >&2
    exit 1
}
has_flag runtime || {
    echo "sign-macos.sh: $binary lacks the hardened runtime (flags: ${flags:-none})" >&2
    exit 1
}
echo "sign-macos.sh: $binary is signed ad hoc with the hardened runtime"
