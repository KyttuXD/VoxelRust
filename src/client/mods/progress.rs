//! The line shown while a server's mods are being downloaded.

use crate::client::render::{Color, Renderer};
use alloc::string::String;

/// `DOWNLOADING MODS 3/12`.
pub fn line(done: usize, total: usize) -> String {
    alloc::format!("DOWNLOADING MODS {}/{}", done.min(total), total)
}

/// Draw it: under the loading bar while the world loads (`loading`), else
/// small at the top of the screen.
pub fn draw(r: &mut Renderer, w: f32, h: f32, s: f32, done: usize, total: usize, loading: bool) {
    let text = line(done, total);
    let tw = Renderer::text_width(&text, s);
    let y = if loading { h * 0.5 + 24.0 * s } else { 6.0 * s };
    let white: Color = [255, 255, 255, 255];
    r.label((w - tw) * 0.5, y, s, &text, white);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_line_counts_mods() {
        assert_eq!(super::line(3, 12), "DOWNLOADING MODS 3/12");
        assert_eq!(super::line(13, 12), "DOWNLOADING MODS 12/12");
    }
}
