#!/usr/bin/env bash
# Builds Linux release binaries and packages each as landing_linux_<arch>.tar.gz,
# the naming the install script expects. Linux-only.
#
# Binaries are dynamically linked against glibc (gnu targets). Fully static musl
# is left to a future Docker image; cross-linking musl here is not attempted.
#
# Needs (once):
#   rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu \
#     armv7-unknown-linux-gnueabihf i686-unknown-linux-gnu
#   sudo apt-get install -y gcc-aarch64-linux-gnu gcc-arm-linux-gnueabihf gcc-i686-linux-gnu
set -euo pipefail

cd "$(dirname "$0")/.."

OUT_DIR="${OUT_DIR:-dist}"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

VERSION="${VERSION:-$(git describe --tags --always --dirty 2>/dev/null || echo dev)}"
export LANDING_VERSION_OVERRIDE="${VERSION#v}"

# target triple -> asset arch -> cross C compiler (empty for the native target)
TARGETS=(
  "x86_64-unknown-linux-gnu amd64 "
  "aarch64-unknown-linux-gnu arm64 aarch64-linux-gnu-gcc"
  "armv7-unknown-linux-gnueabihf arm arm-linux-gnueabihf-gcc"
  "i686-unknown-linux-gnu 386 i686-linux-gnu-gcc"
)

for spec in "${TARGETS[@]}"; do
  read -r triple arch cross_cc <<<"$spec"
  name="landing_linux_${arch}"

  if [ -n "${cross_cc:-}" ] && ! command -v "$cross_cc" >/dev/null; then
    echo "skipping ${name}: missing cross compiler ${cross_cc}" >&2
    continue
  fi

  echo "building ${name} (${triple})..."
  if [ -n "${cross_cc:-}" ]; then
    cc_var="CC_${triple//-/_}"
    export "${cc_var}=${cross_cc}"
    linker_var="CARGO_TARGET_$(echo "$triple" | tr 'a-z-' 'A-Z_')_LINKER"
    export "${linker_var}=${cross_cc}"
  fi

  cargo build --release --locked --target "$triple" --bin landing

  build_dir="$(mktemp -d)"
  cp "target/${triple}/release/landing" "${build_dir}/landing"
  tar -C "$build_dir" -czf "${OUT_DIR}/${name}.tar.gz" landing
  rm -rf "$build_dir"
done

if [ -z "${SKIP_SUMS:-}" ]; then
  ( cd "$OUT_DIR"; sha256sum ./*.tar.gz > SHA256SUMS )
fi

ls -la "$OUT_DIR"
