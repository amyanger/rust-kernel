/// Interactive shell — a read-eval-print loop on bare hardware.
///
/// Reads keyboard scancodes from the interrupt handler's queue,
/// decodes them into characters, buffers a command line, and
/// executes commands when Enter is pressed.

extern crate alloc;

use alloc::string::String;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

use crate::console::CONSOLE;
use crate::filesystem::FILESYSTEM;
use crate::framebuffer::FRAMEBUFFER;
use crate::task::keyboard::ScancodeStream;
use crate::vga_buffer::WRITER;

const MAX_CMD_LEN: usize = 256;

pub async fn run() {
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );
    let mut input = String::with_capacity(MAX_CMD_LEN);
    let mut cwd: u64 = 0;
    let scancode_stream = ScancodeStream::new();

    crate::println!();
    // Colored welcome banner
    set_fg_color("cyan");
    crate::println!("========================================");
    crate::println!("         Welcome to RustKernel v0.1     ");
    crate::println!("========================================");
    set_fg_color("white");
    crate::println!("Type 'help' for available commands.");
    crate::println!();
    print_prompt(cwd);

    loop {
        let scancode = scancode_stream.next().await;

        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(character) => match character {
                        '\n' => {
                            crate::println!();
                            execute_command(&input, &mut cwd);
                            input.clear();
                            print_prompt(cwd);
                        }
                        '\u{0008}' => {
                            if !input.is_empty() {
                                input.pop();
                                x86_64::instructions::interrupts::without_interrupts(|| {
                                    WRITER.lock().write_byte(0x08);
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
    }
}

fn print_prompt(cwd: u64) {
    let path = {
        let fs = FILESYSTEM.lock();
        if let Some(fs) = fs.as_ref() {
            fs.get_path(cwd).unwrap_or_else(|_| String::from("/"))
        } else {
            String::from("/")
        }
    };
    crate::print!("{}> ", path);
}

fn execute_command(cmd: &str, cwd: &mut u64) {
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
            crate::println!("  help              - Show this help message");
            crate::println!("  echo <text>       - Print text to screen");
            crate::println!("  clear             - Clear the screen");
            crate::println!("  info              - Show system information");
            crate::println!("  halt              - Halt the CPU");
            crate::println!("  panic             - Trigger a kernel panic");
            crate::println!("  page <addr>       - Show page table info for hex address");
            crate::println!("  color <name>      - Set text color (white/red/green/blue/cyan/yellow/magenta)");
            crate::println!("  ls [path]          - List directory contents");
            crate::println!("  cat <path>         - Print file contents");
            crate::println!("  touch <path>       - Create empty file");
            crate::println!("  mkdir <path>       - Create directory");
            crate::println!("  rm <path>          - Remove file or empty directory");
            crate::println!("  cd [path]          - Change directory (no args = root)");
            crate::println!("  pwd                - Print working directory");
            crate::println!("  write <path> <txt> - Write text to file");
            crate::println!("  draw rect <x> <y> <w> <h> <color>");
            crate::println!("  draw line <x1> <y1> <x2> <y2> <color>");
            crate::println!("  draw circle <cx> <cy> <r> <color>");
            crate::println!("  screenfill <color> - Fill screen with color");
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
            let fb = FRAMEBUFFER.lock();
            if let Some(f) = fb.as_ref() {
                crate::println!(
                    "Framebuffer: {}x{}, {} bpp",
                    f.width(),
                    f.height(),
                    f.info().bytes_per_pixel
                );
            }
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
        "color" => {
            if args.is_empty() {
                crate::println!("Usage: color <name>");
                crate::println!("Colors: white, red, green, blue, cyan, yellow, magenta");
                return;
            }
            if set_fg_color(args) {
                crate::println!("Color set to {}", args);
            } else {
                crate::println!("Unknown color: {}", args);
                crate::println!("Colors: white, red, green, blue, cyan, yellow, magenta");
            }
        }
        "draw" => {
            cmd_draw(args);
        }
        "ls" => {
            let target = if args.is_empty() { "." } else { args };
            let mut fs = FILESYSTEM.lock();
            if let Some(fs) = fs.as_mut() {
                match fs.resolve_path(target, *cwd) {
                    Ok(dir_id) => match fs.list_dir(dir_id) {
                        Ok(entries) => {
                            crate::println!(".   ../");
                            for (name, is_dir) in entries {
                                if is_dir {
                                    crate::println!("{}/", name);
                                } else {
                                    crate::println!("{}", name);
                                }
                            }
                        }
                        Err(e) => crate::println!("ls: {}", e),
                    },
                    Err(e) => crate::println!("ls: {}", e),
                }
            }
        }
        "cat" => {
            if args.is_empty() {
                crate::println!("Usage: cat <path>");
                return;
            }
            let fs = FILESYSTEM.lock();
            if let Some(fs) = fs.as_ref() {
                match fs.resolve_path(args, *cwd) {
                    Ok(inode_id) => match fs.read_file(inode_id) {
                        Ok(data) => {
                            let text = core::str::from_utf8(data).unwrap_or("<binary data>");
                            crate::println!("{}", text);
                        }
                        Err(e) => crate::println!("cat: {}", e),
                    },
                    Err(e) => crate::println!("cat: {}", e),
                }
            }
        }
        "touch" => {
            if args.is_empty() {
                crate::println!("Usage: touch <path>");
                return;
            }
            let mut fs = FILESYSTEM.lock();
            if let Some(fs) = fs.as_mut() {
                match fs.create_file(args, *cwd) {
                    Ok(_) => {}
                    Err(crate::filesystem::FsError::AlreadyExists) => {} // idempotent
                    Err(e) => crate::println!("touch: {}", e),
                }
            }
        }
        "mkdir" => {
            if args.is_empty() {
                crate::println!("Usage: mkdir <path>");
                return;
            }
            let mut fs = FILESYSTEM.lock();
            if let Some(fs) = fs.as_mut() {
                match fs.create_dir(args, *cwd) {
                    Ok(_) => {}
                    Err(e) => crate::println!("mkdir: {}", e),
                }
            }
        }
        "rm" => {
            if args.is_empty() {
                crate::println!("Usage: rm <path>");
                return;
            }
            let mut fs = FILESYSTEM.lock();
            if let Some(fs) = fs.as_mut() {
                match fs.remove(args, *cwd) {
                    Ok(()) => {}
                    Err(e) => crate::println!("rm: {}", e),
                }
            }
        }
        "cd" => {
            let target = if args.is_empty() { "/" } else { args };
            let fs = FILESYSTEM.lock();
            if let Some(fs) = fs.as_ref() {
                match fs.resolve_path(target, *cwd) {
                    Ok(inode_id) => {
                        if fs.is_directory(inode_id) {
                            *cwd = inode_id;
                        } else {
                            crate::println!("cd: not a directory");
                        }
                    }
                    Err(e) => crate::println!("cd: {}", e),
                }
            }
        }
        "pwd" => {
            let fs = FILESYSTEM.lock();
            if let Some(fs) = fs.as_ref() {
                match fs.get_path(*cwd) {
                    Ok(path) => crate::println!("{}", path),
                    Err(e) => crate::println!("pwd: {}", e),
                }
            }
        }
        "write" => {
            if args.is_empty() {
                crate::println!("Usage: write <path> <text>");
                return;
            }
            let (path, text) = match args.split_once(' ') {
                Some((p, t)) => (p, t),
                None => {
                    crate::println!("Usage: write <path> <text>");
                    return;
                }
            };
            let mut fs = FILESYSTEM.lock();
            if let Some(fs) = fs.as_mut() {
                match fs.write_file(path, text.as_bytes(), *cwd) {
                    Ok(()) => {}
                    Err(e) => crate::println!("write: {}", e),
                }
            }
        }
        "screenfill" => {
            if args.is_empty() {
                crate::println!("Usage: screenfill <color>");
                return;
            }
            if let Some((r, g, b)) = parse_color(args) {
                // Only lock FRAMEBUFFER — don't reset console cursor,
                // this is a raw drawing command. Use `clear` to reset text.
                let mut fb = FRAMEBUFFER.lock();
                if let Some(f) = fb.as_mut() {
                    f.clear(r, g, b);
                }
            } else {
                crate::println!("Unknown color: {}", args);
            }
        }
        _ => {
            crate::println!("Unknown command: {}", command);
            crate::println!("Type 'help' for available commands.");
        }
    }
}

fn cmd_draw(args: &str) {
    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        crate::println!("Usage: draw rect|line|circle <params> <color>");
        return;
    }

    match parts[0] {
        "rect" => {
            // draw rect <x> <y> <w> <h> <color>
            if parts.len() < 6 {
                crate::println!("Usage: draw rect <x> <y> <w> <h> <color>");
                return;
            }
            let (x, y, w, h) = match (
                parts[1].parse::<usize>(),
                parts[2].parse::<usize>(),
                parts[3].parse::<usize>(),
                parts[4].parse::<usize>(),
            ) {
                (Ok(x), Ok(y), Ok(w), Ok(h)) => (x, y, w, h),
                _ => {
                    crate::println!("Invalid coordinates");
                    return;
                }
            };
            if let Some((r, g, b)) = parse_color(parts[5]) {
                let mut fb = FRAMEBUFFER.lock();
                if let Some(f) = fb.as_mut() {
                    f.fill_rect(x, y, w, h, r, g, b);
                }
            } else {
                crate::println!("Unknown color: {}", parts[5]);
            }
        }
        "line" => {
            // draw line <x1> <y1> <x2> <y2> <color>
            if parts.len() < 6 {
                crate::println!("Usage: draw line <x1> <y1> <x2> <y2> <color>");
                return;
            }
            let (x1, y1, x2, y2) = match (
                parts[1].parse::<i32>(),
                parts[2].parse::<i32>(),
                parts[3].parse::<i32>(),
                parts[4].parse::<i32>(),
            ) {
                (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
                _ => {
                    crate::println!("Invalid coordinates");
                    return;
                }
            };
            if let Some((r, g, b)) = parse_color(parts[5]) {
                let mut fb = FRAMEBUFFER.lock();
                if let Some(f) = fb.as_mut() {
                    f.draw_line(x1, y1, x2, y2, r, g, b);
                }
            } else {
                crate::println!("Unknown color: {}", parts[5]);
            }
        }
        "circle" => {
            // draw circle <cx> <cy> <r> <color>
            if parts.len() < 5 {
                crate::println!("Usage: draw circle <cx> <cy> <r> <color>");
                return;
            }
            let (cx, cy, radius) = match (
                parts[1].parse::<i32>(),
                parts[2].parse::<i32>(),
                parts[3].parse::<i32>(),
            ) {
                (Ok(a), Ok(b), Ok(c)) => (a, b, c),
                _ => {
                    crate::println!("Invalid coordinates");
                    return;
                }
            };
            if let Some((r, g, b)) = parse_color(parts[4]) {
                let mut fb = FRAMEBUFFER.lock();
                if let Some(f) = fb.as_mut() {
                    f.draw_circle(cx, cy, radius, r, g, b);
                }
            } else {
                crate::println!("Unknown color: {}", parts[4]);
            }
        }
        _ => {
            crate::println!("Unknown draw command: {}", parts[0]);
            crate::println!("Shapes: rect, line, circle");
        }
    }
}

fn parse_color(name: &str) -> Option<(u8, u8, u8)> {
    match name {
        "white" => Some((255, 255, 255)),
        "red" => Some((255, 0, 0)),
        "green" => Some((0, 255, 0)),
        "blue" => Some((0, 0, 255)),
        "cyan" => Some((0, 255, 255)),
        "yellow" => Some((255, 255, 0)),
        "magenta" => Some((255, 0, 255)),
        "black" => Some((0, 0, 0)),
        "gray" | "grey" => Some((128, 128, 128)),
        "orange" => Some((255, 165, 0)),
        _ => None,
    }
}

fn set_fg_color(name: &str) -> bool {
    if let Some((r, g, b)) = parse_color(name) {
        let mut console = CONSOLE.lock();
        if let Some(c) = console.as_mut() {
            c.set_fg(r, g, b);
        }
        true
    } else {
        false
    }
}
