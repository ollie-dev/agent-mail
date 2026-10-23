#!/bin/sh
# agent-mail installer for macOS.
#
#   curl -fsSL https://github.com/ollie-dev/agent-mail/releases/latest/download/install.sh | sh
#
# Downloads the latest release for this Mac's architecture, verifies its
# SHA-256 checksum, and only then installs the binary to ~/.local/bin.
# Never uses sudo and never runs what it downloads. The download location
# can be overridden with AGENT_MAIL_DOWNLOAD_URL.
set -eu

base_url=${AGENT_MAIL_DOWNLOAD_URL:-https://github.com/ollie-dev/agent-mail/releases/latest/download}
bin_dir="$HOME/.local/bin"
dest="$bin_dir/agent-mail"

fail() {
    echo "agent-mail installer: $*" >&2
    exit 1
}

if [ "$(uname -s)" != "Darwin" ]; then
    fail "agent-mail releases are available for macOS only"
fi
case "$(uname -m)" in
    arm64 | aarch64) target=aarch64-apple-darwin ;;
    x86_64) target=x86_64-apple-darwin ;;
    *) fail "unsupported architecture $(uname -m); agent-mail releases are built for arm64 and x86_64" ;;
esac

asset="agent-mail-$target.tar.gz"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/agent-mail-install.XXXXXX") || fail "cannot create a temporary directory"
staged=""
cleanup() {
    rm -rf "$tmp"
    if [ -n "$staged" ]; then
        rm -f "$staged"
    fi
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

echo "Downloading $asset ..."
curl -fsSL "$base_url/$asset" -o "$tmp/$asset" || fail "could not download $base_url/$asset"
curl -fsSL "$base_url/$asset.sha256" -o "$tmp/$asset.sha256" ||
    fail "could not download $base_url/$asset.sha256"

# The checksum file must name exactly this asset, so the check below cannot
# be pointed at another file.
expected=$(cat "$tmp/$asset.sha256")
if ! printf '%s\n' "$expected" | grep -Eqx "[0-9a-f]{64}  $asset"; then
    fail "checksum file for $asset is malformed; nothing was installed"
fi

if command -v shasum >/dev/null 2>&1; then
    check() { shasum -a 256 -c "$asset.sha256"; }
elif command -v sha256sum >/dev/null 2>&1; then
    check() { sha256sum -c "$asset.sha256"; }
else
    fail "neither shasum nor sha256sum is available to verify the checksum; nothing was installed"
fi
if ! (cd "$tmp" && check >/dev/null 2>&1); then
    fail "checksum mismatch for $asset; nothing was installed"
fi
echo "Checksum verified."

# Only now is anything extracted, and only the one expected member.
mkdir "$tmp/extract"
tar -xzf "$tmp/$asset" -C "$tmp/extract" agent-mail 2>/dev/null ||
    fail "$asset does not hold agent-mail; nothing was installed"
binary="$tmp/extract/agent-mail"
if [ ! -f "$binary" ] || [ -L "$binary" ]; then
    fail "$asset does not hold a regular agent-mail file; nothing was installed"
fi

mkdir -p "$bin_dir"
# Stage next to the destination so the final mv is an atomic rename.
staged="$bin_dir/.agent-mail.install.$$"
cp "$binary" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$dest"
staged=""

echo "Installed agent-mail to $dest"
case ":${PATH:-}:" in
    *":$bin_dir:"*) ;;
    *)
        echo "Note: $bin_dir is not on your PATH. Add it, for example:"
        echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc"
        ;;
esac
