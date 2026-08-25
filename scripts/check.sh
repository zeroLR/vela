#!/bin/bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

cargo fmt --manifest-path "$repo_root/core/Cargo.toml" --all --check
cargo clippy --manifest-path "$repo_root/core/Cargo.toml" --workspace --all-targets -- -D warnings
cargo build --manifest-path "$repo_root/core/Cargo.toml" --workspace
cargo test --manifest-path "$repo_root/core/Cargo.toml" --workspace
VELA_CORE_PATH="$repo_root/core/target/debug/vela-core" swift test --package-path "$repo_root/app"
