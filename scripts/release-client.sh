#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/release-client.sh [windows|linux|mac]

Builds the client in release mode, prepares a release folder in dist/,
and creates a zip archive containing:
- client binary
- assets/
- config/
- .env
- README.md

If no argument is provided, the default target is linux.
EOF
}

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    return 1
  fi
  return 0
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
    bin_name="oplexa-game"
    ;;
  windows|win|w)
    platform="windows"
    target_triple="x86_64-pc-windows-gnu"
    bin_name="oplexa-game.exe"
    ;;
  mac|macos|darwin)
    platform="mac"
    target_triple="aarch64-apple-darwin"
    bin_name="oplexa-game"
    ;;
  *)
    echo "Unsupported platform: $platform_raw" >&2
    usage
    exit 1
    ;;
esac

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"
dist_root="${root_dir}/dist/${platform}"
package_name="oplexa-game-client-${platform}"
package_dir="${dist_root}/${package_name}"
zip_path="${dist_root}/${package_name}.zip"

if ! command -v python3 >/dev/null 2>&1; then
  echo "'python3' command not found. Please install python3 first." >&2
  exit 1
fi

missing_tools=()

case "$platform" in
  windows)
    if ! require_cmd "x86_64-w64-mingw32-gcc"; then
      missing_tools+=("x86_64-w64-mingw32-gcc")
    fi
    if ! require_cmd "x86_64-w64-mingw32-dlltool"; then
      missing_tools+=("x86_64-w64-mingw32-dlltool")
    fi
    export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-x86_64-w64-mingw32-gcc}"
    ;;
  mac)
    if ! require_cmd "aarch64-apple-darwin-clang"; then
      missing_tools+=("aarch64-apple-darwin-clang")
    fi
    export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="${CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER:-aarch64-apple-darwin-clang}"
    ;;
esac

if [[ "${#missing_tools[@]}" -gt 0 ]]; then
  echo "Cannot build target '${target_triple}' because required cross-compile tools are missing." >&2
  printf 'Missing: %s\n' "${missing_tools[*]}" >&2
  echo >&2
  if [[ "$platform" == "windows" ]]; then
    echo "Install MinGW toolchain first." >&2
    echo "Debian/Ubuntu: sudo apt install mingw-w64" >&2
    echo "Fedora:        sudo dnf install mingw64-gcc mingw64-binutils" >&2
    echo "Arch:          sudo pacman -S mingw-w64-gcc" >&2
  elif [[ "$platform" == "mac" ]]; then
    echo "Install an osxcross toolchain that provides 'aarch64-apple-darwin-clang'." >&2
  fi
  exit 1
fi

required_entries=("assets" "config" ".env" "README.md")
for entry in "${required_entries[@]}"; do
  if [[ ! -e "${root_dir}/${entry}" ]]; then
    echo "Required entry not found: ${root_dir}/${entry}" >&2
    exit 1
  fi
done

echo "==> Target platform: ${platform} (${target_triple})"
echo "==> Ensuring Rust target is installed..."
rustup target add "$target_triple" >/dev/null

echo "==> Building release client..."
cargo build \
  --manifest-path "${root_dir}/Cargo.toml" \
  -p oplexa-game \
  --release \
  --target "$target_triple"

artifact_path="${root_dir}/target/${target_triple}/release/${bin_name}"
if [[ ! -f "$artifact_path" ]]; then
  echo "Build finished, but artifact was not found at: $artifact_path" >&2
  exit 1
fi

rm -rf "$package_dir" "$zip_path"
mkdir -p "$package_dir"

cp "$artifact_path" "${package_dir}/${bin_name}"
cp -a "${root_dir}/assets" "${package_dir}/assets"
cp -a "${root_dir}/config" "${package_dir}/config"
cp "${root_dir}/.env" "${package_dir}/.env"
cp "${root_dir}/README.md" "${package_dir}/README.md"

echo "==> Creating zip archive..."
python3 - "$package_dir" "$zip_path" <<'PY'
import pathlib
import sys
import zipfile

source_dir = pathlib.Path(sys.argv[1]).resolve()
zip_file = pathlib.Path(sys.argv[2]).resolve()

with zipfile.ZipFile(zip_file, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    for path in sorted(source_dir.rglob("*")):
        if path.is_file():
            zf.write(path, path.relative_to(source_dir))
PY

echo "==> Done."
echo "Folder: ${package_dir}"
echo "Archive: ${zip_path}"
