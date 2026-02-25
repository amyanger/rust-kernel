# RustKernel

A minimal bare-metal x86_64 kernel written entirely in Rust. Boots on QEMU and implements core OS functionality from scratch — interrupt handling, memory management, heap allocation, and an interactive shell.

## Features

- **Bare-metal x86_64** — Runs directly on hardware (or QEMU) with no OS underneath. Uses `#![no_std]` throughout.
- **Bootloader integration** — Uses `bootloader_api 0.11` for BIOS boot with dynamic physical memory mapping.
- **Interrupt handling** — Full IDT setup with handlers for CPU exceptions (breakpoint, double fault, page fault) and hardware interrupts (timer, keyboard) via the 8259 PIC.
- **4-level paging** — x86_64 page table hierarchy (PML4 → PDPT → PD → PT) with an offset page table mapper and boot-info-based frame allocator.
- **Heap allocation** — 256 KiB heap using a linked-list free-list allocator, enabling `Box`, `Vec`, `String`, and other `alloc` types.
- **Serial I/O** — UART 16550 driver on COM1 for debug output, redirected to the host terminal via QEMU.
- **Interactive shell** — Command-line interface with keyboard input (US QWERTY layout) supporting `help`, `echo`, `clear`, `info`, `halt`, `panic`, and `page` commands.
- **Double-fault safety** — Dedicated IST stack for the double-fault handler prevents triple faults on stack overflow.
- **Integration tests** — Custom test framework running under QEMU with tests for boot, heap allocation, and stack overflow handling.

## Project Structure

```
rust-kernel/
├── Cargo.toml                # Package manifest & dependencies
├── Cargo.lock                # Dependency lock file
├── Makefile                  # Build, run, test, clean targets
├── run.sh                    # Build + QEMU launcher script
├── rust-toolchain.toml       # Nightly toolchain configuration
├── .cargo/
│   └── config.toml           # Build target & build-std settings
├── src/
│   ├── main.rs               # Kernel entry point & initialization
│   ├── lib.rs                # Library root & custom test harness
│   ├── serial.rs             # UART 16550 serial port driver
│   ├── vga_buffer.rs         # Text output (routed through serial)
│   ├── gdt.rs                # Global Descriptor Table & TSS setup
│   ├── interrupts.rs         # IDT, PIC, exception & IRQ handlers
│   ├── memory.rs             # Paging setup & frame allocation
│   ├── allocator.rs          # Heap allocator initialization
│   └── shell.rs              # Interactive command shell
└── tests/
    ├── basic_boot.rs         # Boot & serial printing tests
    ├── heap_allocation.rs    # Heap allocation correctness tests
    └── stack_overflow.rs     # Double-fault handler verification
```

## Prerequisites

- **Rust nightly** — Required for unstable features (`abi_x86_interrupt`, `custom_test_frameworks`, `-Z build-std`). The `rust-toolchain.toml` handles this automatically.
- **QEMU** — `qemu-system-x86_64` for running and testing the kernel.
- **Standard build tools** — `make`, a working shell.

### Install QEMU

```bash
# macOS
brew install qemu

# Ubuntu/Debian
sudo apt install qemu-system-x86

# Arch Linux
sudo pacman -S qemu-system-x86
```

## Building & Running

### Using the run script

```bash
# Build the kernel, create a bootable disk image, and launch QEMU
./run.sh

# Build only (no QEMU)
./run.sh build
```

### Using Make

```bash
make build    # Compile the kernel
make run      # Build + boot in QEMU
make test     # Run integration tests
make clean    # Remove build artifacts
```

### Direct Cargo

```bash
cargo build   # Build (uses .cargo/config.toml for target & build-std)
cargo test    # Run tests
```

## Shell Commands

Once the kernel boots, you'll be dropped into an interactive shell:

| Command | Description |
|---------|-------------|
| `help` | Show available commands |
| `echo <text>` | Print text back to the console |
| `clear` | Clear the screen |
| `info` | Display system info (architecture, heap size & address) |
| `halt` | Halt the CPU |
| `panic` | Trigger a kernel panic (for testing) |
| `page <hex>` | Show page table index breakdown for a virtual address |

## Architecture Overview

### Boot Sequence

1. Bootloader loads the kernel in 64-bit long mode with physical memory mapped at a known offset
2. Early serial write (sanity check)
3. GDT + TSS initialization (sets up interrupt stacks)
4. IDT + PIC initialization (enables hardware interrupts)
5. Page table setup using bootloader-provided physical memory offset
6. Heap allocation (256 KiB mapped at `0x4444_4444_0000`)
7. Shell launch — keyboard-driven REPL

### Memory Layout

- **Physical memory**: Identity-mapped by bootloader at a configurable offset
- **Heap**: 256 KiB at virtual address `0x4444_4444_0000`
- **Double-fault stack**: 20 KiB dedicated IST entry

### Interrupt Handling

- CPU exceptions: Breakpoint (#3), Double Fault (#8), Page Fault (#14)
- Hardware IRQs: Timer (IRQ0), Keyboard (IRQ1) via 8259 PIC remapped to IDT entries 32–47
- Keyboard scancodes buffered in a 128-byte circular queue, decoded in the shell

## Tests

The project uses a custom test framework that boots the kernel in QEMU and communicates results via the `isa-debug-exit` device:

- **basic_boot** — Verifies the kernel boots and serial output works
- **heap_allocation** — Tests `Box`, `Vec`, and repeated allocation up to heap capacity
- **stack_overflow** — Triggers infinite recursion and verifies the double-fault handler catches it cleanly

Run tests with:

```bash
make test
# or
cargo test
```

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `bootloader_api` | 0.11 | Boot protocol & BootInfo struct |
| `spin` | 0.9 | Spinlock-based synchronization |
| `uart_16550` | 0.4 | Serial port driver |
| `x86_64` | 0.15 | CPU registers, paging, port I/O |
| `pic8259` | 0.11 | 8259 PIC interrupt controller |
| `pc-keyboard` | 0.7 | Scancode-to-key decoding |
| `linked_list_allocator` | 0.10 | Free-list heap allocator |

## Resources

This project draws from and builds upon ideas from:

- [Writing an OS in Rust](https://os.phil-opp.com/) by Philipp Oppermann
- [OSDev Wiki](https://wiki.osdev.org/)
- [The Rust `x86_64` crate documentation](https://docs.rs/x86_64/)

## License

This project is provided as-is for educational purposes.
