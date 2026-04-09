#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/release-server.sh [windows|linux|mac]

Builds the dedicated server in release mode and copies the artifact to dist/.
If no argument is provided, the default target is linux.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

platform_raw="${1:-linux}"
platform="$(printf '%s' "$platform_raw" | tr '[:upper:]' '[:lower:]')"

case "$platform" in
  linux|lin)
    platform="linux"
    target_triple="x86_64-unknown-linux-gnu"
    bin_name="oplexa-game-server"
    ;;
  windows|win|w)
    platform="windows"
    target_triple="x86_64-pc-windows-gnu"
    bin_name="oplexa-game-server.exe"
    ;;
  mac|macos|darwin)
    platform="mac"
    target_triple="aarch64-apple-darwin"
    bin_name="oplexa-game-server"
    ;;
  *)
    echo "Unsupported platform: $platform_raw" >&2
    usage
    exit 1
    ;;
esac

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"
dist_dir="${root_dir}/dist/${platform}"

echo "==> Target platform: ${platform} (${target_triple})"
echo "==> Ensuring Rust target is installed..."
rustup target add "$target_triple" >/dev/null

echo "==> Building release server..."
cargo build \
  --manifest-path "${root_dir}/Cargo.toml" \
  -p oplexa-game-server \
  --release \
  --target "$target_triple"

artifact_path="${root_dir}/target/${target_triple}/release/${bin_name}"
if [[ ! -f "$artifact_path" ]]; then
  echo "Build finished, but artifact was not found at: $artifact_path" >&2
  exit 1
fi

mkdir -p "$dist_dir"
cp "$artifact_path" "${dist_dir}/${bin_name}"

if [[ -f "${root_dir}/server.settings.toml" ]]; then
  cp "${root_dir}/server.settings.toml" "${dist_dir}/server.settings.toml"
fi

echo "==> Done."
echo "Artifact: ${dist_dir}/${bin_name}"
