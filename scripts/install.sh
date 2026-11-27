#!/bin/sh
# Installs the latest (or a specific) Landing release for Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/tweedge/landing/main/scripts/install.sh | sh
#
# Override the version or install location with environment variables:
#
#   VERSION=v1.0.0 INSTALL_DIR=~/bin sh install.sh
set -e

REPO="tweedge/landing"
BIN="landing"
VERSION="${VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-}"

say() { echo "landing install: $*"; }
die() {
  echo "landing install: $*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "this script needs '$1', which isn't on your PATH"
}
need curl
need tar

os="$(uname -s)"
case "$os" in
  Linux) os=linux ;;
  *) die "unsupported OS '$os'. This script only installs Linux binaries; download a release manually from https://github.com/${REPO}/releases" ;;
esac

arch="$(uname -m)"
case "$arch" in
  x86_64 | amd64) arch=amd64 ;;
  aarch64 | arm64) arch=arm64 ;;
  armv6l | armv7l | arm) arch=arm ;;
  i386 | i686) arch=386 ;;
  *) die "unsupported architecture '$arch'. Download a release manually from https://github.com/${REPO}/releases" ;;
esac

if [ "$VERSION" = "latest" ]; then
  say "checking for the latest release..."
  version="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d '"' -f4)"
  [ -n "$version" ] || die "could not determine the latest release from the GitHub API"
else
  version="$VERSION"
fi

asset="${BIN}_${os}_${arch}.tar.gz"
url="https://github.com/${REPO}/releases/download/${version}/${asset}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT INT TERM

say "downloading ${url}"
curl -fsSL "$url" -o "${tmpdir}/${asset}" || die "download failed - is '${version}' a real release, and does it have a ${os}/${arch} build?"

# Verify the checksum when the release ships SHA256SUMS.
if curl -fsSL "https://github.com/${REPO}/releases/download/${version}/SHA256SUMS" -o "${tmpdir}/SHA256SUMS" 2>/dev/null; then
  expected="$(grep "  ${asset}\$" "${tmpdir}/SHA256SUMS" | cut -d ' ' -f1)"
  if [ -n "$expected" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
      actual="$(sha256sum "${tmpdir}/${asset}" | cut -d ' ' -f1)"
    else
      actual="$(shasum -a 256 "${tmpdir}/${asset}" | cut -d ' ' -f1)"
    fi
    [ "$actual" = "$expected" ] || die "checksum mismatch for ${asset}"
    say "checksum verified"
  fi
fi

tar -C "$tmpdir" -xzf "${tmpdir}/${asset}"
[ -f "${tmpdir}/${BIN}" ] || die "downloaded archive did not contain a ${BIN} binary"

if [ -z "$INSTALL_DIR" ]; then
  if [ "$(id -u)" = "0" ] || [ -w /usr/local/bin ]; then
    INSTALL_DIR=/usr/local/bin
  else
    INSTALL_DIR="${HOME}/.local/bin"
  fi
fi

mkdir -p "$INSTALL_DIR"
mv "${tmpdir}/${BIN}" "${INSTALL_DIR}/${BIN}"
chmod +x "${INSTALL_DIR}/${BIN}"

say "installed ${version} to ${INSTALL_DIR}/${BIN}"

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *) say "note: ${INSTALL_DIR} isn't on your PATH. Add it, e.g.: export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
esac

say "next: run '${BIN} init' in your project directory, then '${BIN}' to install and serve"
