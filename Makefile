# Rust Kernel — Build & Run
#
# Usage:
#   make run      — Build kernel + create disk image + launch QEMU
#   make build    — Build the kernel only
#   make test     — Run integration tests in QEMU
#   make clean    — Remove build artifacts

.PHONY: build run test clean

KERNEL_BIN = target/x86_64-unknown-none/debug/kernel

build:
	cargo build

run: build
	cd runner && cargo run -- $(ARGS)

test:
	cargo test

clean:
	cargo clean
	cd runner && cargo clean
