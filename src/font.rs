/// Bitmap font renderer.
///
/// Embeds a VGA-compatible 8x16 font (256 glyphs, Code Page 437).
/// Each glyph is 16 bytes — one byte per row, MSB = leftmost pixel.

use crate::framebuffer::Framebuffer;

const FONT_DATA: &[u8] = include_bytes!("font_8x16.bin");

pub const CHAR_WIDTH: usize = 8;
pub const CHAR_HEIGHT: usize = 16;

/// Draw a single character at pixel position (x, y) with foreground and background colors.
pub fn draw_char(
    fb: &mut Framebuffer,
    x: usize,
    y: usize,
    c: u8,
    fg: (u8, u8, u8),
    bg: (u8, u8, u8),
) {
    let glyph_offset = (c as usize) * CHAR_HEIGHT;

    for row in 0..CHAR_HEIGHT {
        let byte = FONT_DATA[glyph_offset + row];
        for col in 0..CHAR_WIDTH {
            let pixel_on = (byte >> (7 - col)) & 1 != 0;
            let (r, g, b) = if pixel_on { fg } else { bg };
            fb.put_pixel(x + col, y + row, r, g, b);
        }
    }
}
