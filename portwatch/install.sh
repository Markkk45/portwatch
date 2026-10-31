#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
bin_dir="${HOME}/.local/bin"

command -v cargo >/dev/null 2>&1 || {
  echo "portwatch: cargo is required (install Rust from https://rustup.rs/)" >&2
  exit 1
}

echo "Building portwatch in release mode..."
cargo build --release --manifest-path "${project_dir}/Cargo.toml"

mkdir -p "${bin_dir}"
install -Dm755 "${project_dir}/target/release/portwatch" "${bin_dir}/portwatch"

echo "Installed ${bin_dir}/portwatch"
if [[ ":${PATH}:" != *":${bin_dir}:"* ]]; then
  echo "Add ${bin_dir} to PATH, then run: portwatch"
else
  echo "Run: portwatch"
fi
