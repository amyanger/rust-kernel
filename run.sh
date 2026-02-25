#!/bin/bash
# Build the kernel, create a disk image, and launch QEMU.
#
# Usage:
#   ./run.sh          Build + run in QEMU
#   ./run.sh build    Build only (no QEMU)

set -euo pipefail

# Force nightly toolchain and the correct cargo binary
export PATH="$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:$PATH"
export RUSTUP_TOOLCHAIN=nightly
export CARGO="$(rustup which cargo)"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

KERNEL_BIN="$SCRIPT_DIR/target/x86_64-unknown-none/debug/kernel"

echo "==> Building kernel..."
cargo build

if [ "${1:-}" = "build" ]; then
    echo "==> Kernel built: $KERNEL_BIN"
    exit 0
fi

echo "==> Creating disk image..."

# Build the image creator in an isolated directory (outside project tree
# to avoid inheriting .cargo/config.toml's build-std and target settings)
IMGBUILDER_DIR="/tmp/rust-kernel-imgbuilder"
mkdir -p "$IMGBUILDER_DIR/src"

cat > "$IMGBUILDER_DIR/Cargo.toml" << 'TOML'
[package]
name = "imgbuilder"
version = "0.1.0"
edition = "2021"

[dependencies]
bootloader = "0.11"
TOML

cat > "$IMGBUILDER_DIR/src/main.rs" << 'RUST'
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let kernel_path = PathBuf::from(&args[1]);
    let bios_path = kernel_path.with_extension("bios.img");

    println!("Creating BIOS disk image...");
    bootloader::BiosBoot::new(&kernel_path)
        .create_disk_image(&bios_path)
        .expect("Failed to create BIOS disk image");
    println!("Disk image created: {}", bios_path.display());
}
RUST

# Also need rust-toolchain.toml so it uses nightly
cat > "$IMGBUILDER_DIR/rust-toolchain.toml" << 'TOML'
[toolchain]
channel = "nightly"
TOML

cd "$IMGBUILDER_DIR"
cargo build 2>&1 | grep -v "^$" | tail -5

IMGBUILDER_BIN="$IMGBUILDER_DIR/target/debug/imgbuilder"
"$IMGBUILDER_BIN" "$KERNEL_BIN"

echo "==> Launching QEMU..."
exec qemu-system-x86_64 \
    -drive "format=raw,file=${KERNEL_BIN}.bios.img" \
    -serial stdio
