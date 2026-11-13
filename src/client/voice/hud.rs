//! What voice chat shows on screen: a microphone icon while yours is hot,
//! and a speaker icon beside every player who is talking (on their name
//! tag, and in a short list at the left of the screen).

use crate::render::{Color, Renderer, WHITE};
use alloc::string::String;

/// The state of your own microphone, for its icon.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mic {
    /// Not listening: nothing is drawn.
    Off,
    /// Open, waiting for you to speak (voice activation).
    Armed,
    /// Your voice is being sent.
    Live,
}

/// A microphone, 7 x 10 pixels of the interface's size `s`, top left at (x, y).
pub fn mic_icon(r: &mut Renderer, x: f32, y: f32, s: f32, col: Color) {
    let mut p = |px: f32, py: f32, w: f32, h: f32| r.rect(x + px * s, y + py * s, w * s, h * s, col);
    p(2.0, 0.0, 3.0, 5.0); // the capsule
    p(1.0, 4.0, 1.0, 3.0); // the cradle
    p(5.0, 4.0, 1.0, 3.0);
    p(1.0, 7.0, 5.0, 1.0);
    p(3.0, 8.0, 1.0, 1.0); // the stand
    p(2.0, 9.0, 3.0, 1.0);
}

/// A speaker with sound waves, 9 x 7 pixels, top left at (x, y).
pub fn speaker_icon(r: &mut Renderer, x: f32, y: f32, s: f32, col: Color) {
    let mut p = |px: f32, py: f32, w: f32, h: f32| r.rect(x + px * s, y + py * s, w * s, h * s, col);
    p(0.0, 2.0, 2.0, 3.0);
    p(2.0, 1.0, 1.0, 5.0);
    p(3.0, 0.0, 1.0, 7.0);
    p(5.0, 2.0, 1.0, 3.0);
    p(7.0, 1.0, 1.0, 5.0);
}

const LIVE: Color = [96, 232, 118, 255];
const ARMED: Color = [200, 204, 214, 190];
const TALKING: Color = [120, 220, 255, 255];

/// Draw the voice indicators. `talkers` are the upper-case names of the
/// players who are talking now; `tags` the name tags drawn this frame (screen
/// x, y and upper-case name), which get a speaker beside them.
pub fn draw(r: &mut Renderer, s: f32, h: f32, mic: Mic, talkers: &[&str], tags: &[(f32, f32, String)]) {
    // Beside the name tags of those who are talking.
    for (x, y, name) in tags {
        if talkers.iter().any(|t| *t == name.as_str()) {
            let tw = Renderer::text_width(name, s);
            speaker_icon(r, x + tw * 0.5 + 4.0 * s, *y, s, TALKING);
        }
    }
    // At the left of the screen, low enough to clear the frame rate and debug text.
    let (x0, mut y) = (4.0 * s, (h * 0.4).max(90.0 * s));
    if mic != Mic::Off {
        r.rect(x0 - 2.0 * s, y - 2.0 * s, 11.0 * s, 14.0 * s, [0, 0, 0, 120]);
        mic_icon(r, x0 + s, y, s, if mic == Mic::Live { LIVE } else { ARMED });
        y += 16.0 * s;
    }
    for name in talkers.iter().take(8) {
        let tw = Renderer::text_width(name, s);
        r.rect(x0 - 2.0 * s, y - 2.0 * s, tw + 17.0 * s, 11.0 * s, [0, 0, 0, 120]);
        speaker_icon(r, x0, y, s, TALKING);
        r.label(x0 + 12.0 * s, y, s, name, WHITE);
        y += 12.0 * s;
    }
}
