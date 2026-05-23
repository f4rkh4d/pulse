#!/usr/bin/env bash
# pulse installer. downloads the right binary, drops it where your PATH finds it.
#
#   curl -fsSL https://pulse.frkhd.com/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/f4rkh4d/pulse/main/install.sh | sh
#
# honors PULSE_INSTALL_DIR if you want it somewhere specific.

set -eu

REPO="f4rkh4d/pulse"
BINARY="pulse"
VERSION="${PULSE_VERSION:-latest}"

fail() { printf "pulse: %s\n" "$*" >&2; exit 1; }
say()  { printf "  %s\n" "$*"; }

# ── detect platform ────────────────────────────────────────────────
UNAME_S=$(uname -s)
UNAME_M=$(uname -m)

case "$UNAME_S" in
    Linux)  os="linux" ;;
    Darwin) os="macos" ;;
    *) fail "unsupported os: $UNAME_S. open an issue if you want it supported." ;;
esac

case "$UNAME_M" in
    x86_64|amd64) arch="amd64" ;;
    arm64|aarch64) arch="arm64" ;;
    *) fail "unsupported cpu: $UNAME_M" ;;
esac

platform="${os}-${arch}"

# ── resolve version ────────────────────────────────────────────────
if [ "$VERSION" = "latest" ]; then
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep -m1 '"tag_name"' \
        | cut -d '"' -f 4) \
        || fail "could not fetch latest release tag"
fi

asset="pulse-${VERSION}-${platform}.tar.gz"
url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"

say "platform: ${platform}"
say "version:  ${VERSION}"

# ── choose install dir ────────────────────────────────────────────
INSTALL_DIR="${PULSE_INSTALL_DIR:-}"
if [ -z "$INSTALL_DIR" ]; then
    for dir in "$HOME/.local/bin" "/usr/local/bin" "$HOME/bin"; do
        case ":$PATH:" in
            *":$dir:"*) INSTALL_DIR=$dir; break ;;
        esac
    done
fi
[ -z "$INSTALL_DIR" ] && INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR" || fail "cannot create $INSTALL_DIR"
[ -w "$INSTALL_DIR" ] || fail "$INSTALL_DIR not writable. rerun with PULSE_INSTALL_DIR=/somewhere/writable sh install.sh"

say "install:  ${INSTALL_DIR}/${BINARY}"

# ── download + extract ────────────────────────────────────────────
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

printf "  downloading ... "
if ! curl -fsSL "$url" -o "$tmp/pulse.tar.gz"; then
    printf "fail\n"
    fail "could not download $url. maybe the release is still building, try again in a minute."
fi
printf "done\n"

tar -xzf "$tmp/pulse.tar.gz" -C "$tmp"
[ -f "$tmp/pulse" ] || fail "tarball did not contain a pulse binary"
chmod +x "$tmp/pulse"
mv "$tmp/pulse" "${INSTALL_DIR}/${BINARY}"

say "done. run: pulse --help"

# ── warn if install dir isn't on PATH ─────────────────────────────
case ":$PATH:" in
    *":${INSTALL_DIR}:"*) ;;
    *) printf "\n  note: %s isn't on your PATH. add this to your shell rc:\n    export PATH=\"%s:\$PATH\"\n" "$INSTALL_DIR" "$INSTALL_DIR" ;;
esac
