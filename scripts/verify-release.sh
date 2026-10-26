#!/bin/sh
# Verifies a published agent-mail release.
#
#   scripts/verify-release.sh <tag>      e.g. scripts/verify-release.sh v0.1.0
#
# For each macOS target: downloads the tarball and its .sha256 file with
# `gh release download`, checks the checksum, extracts the binary, checks
# that it is signed ad hoc with the hardened runtime, verifies the build
# attestation, and on the matching architecture runs `agent-mail --version`
# and prints `ran agent-mail --version on <target>`. The other target is not
# run; it prints `skipped agent-mail --version on <target>: not the host
# architecture (<host>)`, with `unknown` as the host on other architectures.
# For the Windows x64 zip: checks the checksum, that the zip holds exactly
# `agent-mail.exe`, and the build attestation (no signature check, no run).
# For the Windows installers `install.ps1` and `uninstall.ps1`: downloads
# each asset with `gh release download "$tag" --pattern <name>`, fails with
# `missing asset <name>` if it is absent, and with `<name> differs from <tag>`
# unless it is byte-identical to `git show "<tag>:<name>"`.
# Needs `gh` (authenticated) and, for the signature check, macOS.
set -eu

repo=ollie-dev/agent-mail
targets="aarch64-apple-darwin x86_64-apple-darwin"

[ $# -eq 1 ] || { echo "usage: $0 <tag>" >&2; exit 2; }
tag=$1
root=$(cd "$(dirname "$0")/.." && pwd)

case "$(uname -m)" in
    arm64 | aarch64) host=aarch64-apple-darwin ;;
    x86_64) host=x86_64-apple-darwin ;;
    *) host="" ;;
esac

work=$(mktemp -d "${TMPDIR:-/tmp}/agent-mail-verify.XXXXXX")
trap 'rm -rf "$work"' EXIT

for target in $targets; do
    asset="agent-mail-$target.tar.gz"
    dir="$work/$target"
    mkdir -p "$dir"
    echo "== $asset"
    gh release download "$tag" --repo "$repo" --dir "$dir" \
        --pattern "$asset" --pattern "$asset.sha256"
    [ -f "$dir/$asset" ] || { echo "missing asset $asset" >&2; exit 1; }
    [ -f "$dir/$asset.sha256" ] || { echo "missing asset $asset.sha256" >&2; exit 1; }
    grep -Eqx "[0-9a-f]{64}  $asset" "$dir/$asset.sha256" || {
        echo "$asset.sha256 is not in shasum -a 256 format" >&2
        exit 1
    }
    (cd "$dir" && shasum -a 256 -c "$asset.sha256")
    members=$(tar -tzf "$dir/$asset")
    [ "$members" = "agent-mail" ] || {
        echo "$asset must hold exactly agent-mail, holds: $members" >&2
        exit 1
    }
    tar -xzf "$dir/$asset" -C "$dir"
    "$root/scripts/sign-macos.sh" --check-only "$dir/agent-mail"
    gh attestation verify "$dir/$asset" --repo "$repo"
    if [ "$target" = "$host" ]; then
        version=$("$dir/agent-mail" --version)
        echo "$version"
        [ "$version" = "agent-mail ${tag#v}" ] || {
            echo "expected agent-mail ${tag#v}, got: $version" >&2
            exit 1
        }
        echo "ran agent-mail --version on $target"
    else
        echo "skipped agent-mail --version on $target: not the host architecture (${host:-unknown})"
    fi
done

# Windows x64: checksum, contents and attestation only. The binary is not
# code-signed and is not run here.
asset="agent-mail-x86_64-pc-windows-msvc.zip"
dir="$work/windows"
mkdir -p "$dir"
echo "== $asset"
gh release download "$tag" --repo "$repo" --dir "$dir" \
    --pattern "$asset" --pattern "$asset.sha256"
[ -f "$dir/$asset" ] || { echo "missing asset $asset" >&2; exit 1; }
[ -f "$dir/$asset.sha256" ] || { echo "missing asset $asset.sha256" >&2; exit 1; }
grep -Eqx "[0-9a-f]{64}  $asset" "$dir/$asset.sha256" || {
    echo "$asset.sha256 is not in shasum -a 256 format" >&2
    exit 1
}
(cd "$dir" && shasum -a 256 -c "$asset.sha256")
members=$(unzip -Z1 "$dir/$asset")
[ "$members" = "agent-mail.exe" ] || {
    echo "$asset must hold exactly agent-mail.exe, holds: $members" >&2
    exit 1
}
gh attestation verify "$dir/$asset" --repo "$repo"

# The Windows installers: each asset must be the file at the tag, byte for
# byte.
dir="$work/installers"
mkdir -p "$dir"
for name in install.ps1 uninstall.ps1; do
    echo "== $name"
    gh release download "$tag" --repo "$repo" --dir "$dir" --pattern "$name" || true
    [ -f "$dir/$name" ] || { echo "missing asset $name" >&2; exit 1; }
    git -C "$root" show "$tag:$name" > "$dir/$name.expected"
    cmp -s "$dir/$name" "$dir/$name.expected" || {
        echo "$name differs from $tag" >&2
        exit 1
    }
done

echo "release $tag verified"
