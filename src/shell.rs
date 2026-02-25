/// Interactive shell — a read-eval-print loop on bare hardware.
///
/// Reads keyboard scancodes from the interrupt handler's queue,
/// decodes them into characters, buffers a command line, and
/// executes commands when Enter is pressed.

extern crate alloc;

use alloc::string::String;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

use crate::interrupts::SCANCODE_QUEUE;
use crate::vga_buffer::WRITER;

const MAX_CMD_LEN: usize = 256;

pub fn run() -> ! {
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );
    let mut input = String::with_capacity(MAX_CMD_LEN);

    crate::println!();
    crate::println!("Welcome to RustKernel v0.1");
    crate::println!("Type 'help' for available commands.");
    crate::println!();
    crate::print!("> ");

    loop {
        let scancode = x86_64::instructions::interrupts::without_interrupts(|| {
            SCANCODE_QUEUE.lock().pop()
        });

        if let Some(scancode) = scancode {
            if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                if let Some(key) = keyboard.process_keyevent(key_event) {
                    match key {
                        DecodedKey::Unicode(character) => match character {
                            '\n' => {
                                crate::println!();
                                execute_command(&input);
                                input.clear();
                                crate::print!("> ");
                            }
                            '\u{0008}' => {
                                if !input.is_empty() {
                                    input.pop();
                                    x86_64::instructions::interrupts::without_interrupts(|| {
                                        let mut writer = WRITER.lock();
                                        if writer.column_position > 0 {
                                            writer.column_position -= 1;
                                            writer.write_byte(b' ');
                                            writer.column_position -= 1;
                                        }
                                    });
                                }
                            }
                            c if c.is_ascii() && !c.is_control() => {
                                if input.len() < MAX_CMD_LEN {
                                    input.push(c);
                                    crate::print!("{}", c);
                                }
                            }
                            _ => {}
                        },
                        DecodedKey::RawKey(_) => {}
                    }
                }
            }
        } else {
            x86_64::instructions::hlt();
        }
    }
}

fn execute_command(cmd: &str) {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return;
    }

    let (command, args) = match cmd.split_once(' ') {
        Some((c, a)) => (c, a.trim()),
        None => (cmd, ""),
    };

    match command {
        "help" => {
            crate::println!("Available commands:");
            crate::println!("  help          - Show this help message");
            crate::println!("  echo <text>   - Print text to screen");
            crate::println!("  clear         - Clear the screen");
            crate::println!("  info          - Show system information");
            crate::println!("  halt          - Halt the CPU");
            crate::println!("  panic         - Trigger a kernel panic");
            crate::println!("  page <addr>   - Show page table info for hex address");
        }
        "echo" => {
            crate::println!("{}", args);
        }
        "clear" => {
            x86_64::instructions::interrupts::without_interrupts(|| {
                WRITER.lock().clear_screen();
            });
        }
        "info" => {
            crate::println!("RustKernel v0.1");
            crate::println!("Architecture: x86_64");
            crate::println!(
                "Heap: {} KiB at {:#x}",
                crate::allocator::HEAP_SIZE / 1024,
                crate::allocator::HEAP_START
            );
        }
        "halt" => {
            crate::println!("Halting CPU...");
            crate::hlt_loop();
        }
        "panic" => {
            panic!("User-triggered panic");
        }
        "page" => {
            if args.is_empty() {
                crate::println!("Usage: page <hex_address>");
                crate::println!("Example: page b8000");
                return;
            }
            match u64::from_str_radix(args.trim_start_matches("0x"), 16) {
                Ok(addr) => {
                    crate::println!("Virtual address: {:#x}", addr);
                    crate::println!("Page offset: {}", addr % 4096);
                    crate::println!("PT index:   {}", (addr >> 12) & 0x1FF);
                    crate::println!("PD index:   {}", (addr >> 21) & 0x1FF);
                    crate::println!("PDPT index: {}", (addr >> 30) & 0x1FF);
                    crate::println!("PML4 index: {}", (addr >> 39) & 0x1FF);
                }
                Err(_) => {
                    crate::println!("Invalid hex address: {}", args);
                }
            }
        }
        _ => {
            crate::println!("Unknown command: {}", command);
            crate::println!("Type 'help' for available commands.");
        }
    }
}
