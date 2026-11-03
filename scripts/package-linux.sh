#!/usr/bin/env bash
set -euo pipefail
if [[ $# != 2 ]]; then
  echo "Usage: package-linux.sh SIGNING_KEY_PATH OUTPUT_DIRECTORY" >&2
  exit 2
fi
task_signing_key=$1
task_output=$2
task_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
task_arch=$(uname -m)
case "$task_arch" in x86_64|aarch64) ;; *) echo "Unsupported architecture" >&2; exit 2 ;; esac
command -v zip >/dev/null
command -v minisign >/dev/null
cargo build --locked --release --manifest-path "$task_root/Cargo.toml"
mkdir -p -- "$task_output"
task_archive="$task_output/fatrocu-cli-linux-$task_arch.zip"
if [[ -e "$task_archive" || -e "$task_archive.minisig" ]]; then
  echo "Candidate output already exists" >&2
  exit 1
fi
zip -j -- "$task_archive" "$task_root/target/release/fatrocu-cli"
minisign -Sm "$task_archive" -s "$task_signing_key"
sha256sum -- "$task_archive"
