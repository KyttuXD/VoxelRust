//! SETTINGS > VOICE: the switches for proximity voice chat, the choice of
//! microphone and the level meter.

use super::{codec, Voice};
use crate::settings::Settings;
use crate::ui::{self, Ui};
use alloc::string::String;

/// A close guess at the base-2 logarithm of a positive number (within about 0.01).
fn log2_guess(x: f32) -> f32 {
    let bits = x.to_bits();
    let exp = ((bits >> 23) & 0xFF) as i32 - 127;
    let m = f32::from_bits((bits & 0x007F_FFFF) | 0x3F80_0000); // 1..2
    // A cubic fitted to log2 on 1..2.
    let l = -1.7417939 + (2.8212026 + (-1.4699568 + (0.44717955 - 0.056570851 * m) * m) * m) * m;
    exp as f32 + l
}

/// Where on the meter's bar (0 to 1) a loudness falls: decibels, as every other meter shows
/// them (a full-scale sound is the right end, 54 dB below it the left), so a microphone that
/// is twice as loud moves the bar by the same amount wherever it starts.
pub fn meter_pos(level: f32) -> f32 {
    if !(level > 0.0) {
        // (Nothing, and a number that is not one.)
        return 0.0;
    }
    let db = 6.0206 * log2_guess(level);
    ((db + 54.0) / 54.0).clamp(0.0, 1.0)
}

/// What the tab asks the app to do besides changing settings.
pub struct Asks {
    /// Something in `Settings` changed.
    pub changed: bool,
    /// The player clicked the push-to-talk key: start waiting for a new one.
    pub rebind: bool,
}

/// Draw the tab. `x`, `y`, `bw`, `bh` and `step` are the settings screen's
/// column; `key_label` is the name of the push-to-talk key (or "PRESS A KEY"
/// while it is being changed); `online` and `allowed` say whether voice is
/// possible here at all.
pub fn draw(ui: &mut Ui, st: &mut Settings, voice: &mut Voice, (x, y, bw, bh, step): (f32, f32, f32, f32, f32), key_label: &str, online: bool, allowed: bool) -> Asks {
    let s = ui.s;
    let row = |i: usize| y + i as f32 * step;
    let half = (bw - 4.0 * s) * 0.5;
    let mut asks = Asks { changed: false, rebind: false };
    if ui.clock - voice.devices_at > 3.0 || ui.clock < voice.devices_at {
        voice.refresh_devices(ui.clock);
    }
    asks.changed |= ui.toggle(x, row(0), bw, bh, "HEAR OTHER PLAYERS", &mut st.voice_hear);
    asks.changed |= ui.toggle(x, row(1), bw, bh, "MICROPHONE", &mut st.voice_mic);
    let mode = if st.voice_activation { "TALK BY  < VOICE >" } else { "TALK BY  < KEY >" };
    if ui.button(x, row(2), half, bh, mode) {
        st.voice_activation = !st.voice_activation;
        asks.changed = true;
    }
    let key = if key_label.is_empty() { String::from("KEY") } else { alloc::format!("KEY: {}", key_label) };
    if ui.button(x + half + 4.0 * s, row(2), half, bh, &key) {
        asks.rebind = true;
    }
    asks.changed |= ui.stepper(x, row(3), bw, bh, "VOICE ACTIVATION LEVEL", &mut st.voice_threshold, 0, 10, 1);
    // The microphone: 0 is the system's default, then each one found.
    let mut pick = voice.devices.iter().position(|d| *d == st.voice_device).map_or(0, |i| i as i32 + 1);
    let name: String = if pick == 0 { String::from("DEFAULT") } else { voice.devices[pick as usize - 1].to_ascii_uppercase().chars().take(24).collect() };
    let auto_w = 44.0 * s;
    if ui.stepper_text(x, row(4), bw - auto_w - 3.0 * s, bh, &alloc::format!("INPUT  < {} >", name), &mut pick, 0, voice.devices.len() as i32, 1) {
        st.voice_device = if pick == 0 { String::new() } else { voice.devices[pick as usize - 1].clone() };
        asks.changed = true;
    }
    // Find the microphone by listening: the player speaks and the one that hears it is chosen.
    let searching = voice.detect_left() > 0.0;
    if ui.button(x + bw - auto_w, row(4), auto_w, bh, if searching { "..." } else { "AUTO" }) && !searching {
        voice.refresh_devices(ui.clock);
        voice.start_detect();
    }
    if let Some(found) = voice.take_detected() {
        if found.is_empty() {
            voice.detect_note = String::from("NO MICROPHONE HEARD YOU. CHECK THAT IT IS ON.");
        } else {
            voice.detect_note = alloc::format!("USING: {}", found.to_ascii_uppercase().chars().take(26).collect::<String>());
            st.voice_device = found;
            asks.changed = true;
        }
    }
    asks.changed |= ui.stepper(x, row(5), bw, bh, "VOICE VOLUME", &mut st.voice_volume, 0, 10, 1);
    // The test: the microphone is only listened to while this is on, and
    // only to show the bar. Nothing is sent or kept.
    if ui.button(x, row(6), half, bh, if voice.testing { "STOP TEST" } else { "TEST MICROPHONE" }) {
        voice.testing = !voice.testing;
    }
    let (bx, bw2) = (x + half + 4.0 * s, half);
    ui.r.rect(bx, row(6), bw2, bh, [16, 18, 24, 230]);
    if voice.testing && voice.mic_open() {
        let pos = meter_pos(voice.level);
        let col = if pos < 0.6 { [96, 220, 118, 255] } else if pos < 0.85 { [240, 210, 90, 255] } else { [236, 90, 80, 255] };
        ui.r.rect(bx + s, row(6) + s, (bw2 - 2.0 * s) * pos, bh - 2.0 * s, col);
        if st.voice_activation {
            let t = meter_pos(codec::threshold(st.voice_threshold));
            ui.r.rect(bx + s + (bw2 - 2.0 * s) * t, row(6), s, bh, [255, 255, 255, 255]);
        }
    }
    // What is wrong, if anything.
    let note = if !online {
        "VOICE CHAT WORKS ON MULTIPLAYER SERVERS."
    } else if !allowed {
        "THIS SERVER DOES NOT LET YOU TALK."
    } else if voice.devices.is_empty() {
        "NO MICROPHONE WAS FOUND."
    } else if voice.failed {
        "THE MICROPHONE WOULD NOT OPEN."
    } else if !st.voice_mic {
        "YOUR MICROPHONE IS OFF: YOU CAN HEAR, NOT TALK."
    } else {
        ""
    };
    if searching {
        ui.text(x, row(7) + 2.0 * s, &alloc::format!("SPEAK NOW: LISTENING TO EVERY MICROPHONE... {}", (voice.detect_left() + 0.999) as i32), ui::ACCENT);
    } else if !voice.detect_note.is_empty() && !voice.failed {
        ui.text(x, row(7) + 2.0 * s, &voice.detect_note.clone(), ui::DIM);
    } else if voice.testing && voice.failed {
        ui.text(x, row(7) + 2.0 * s, "THE MICROPHONE WOULD NOT OPEN.", ui::ACCENT);
    } else if !note.is_empty() {
        ui.text(x, row(7) + 2.0 * s, note, ui::ACCENT);
    }
    ui.text(x, row(8), "ONLY PLAYERS NEAR YOU HEAR YOU. THE SERVER PASSES", ui::DIM);
    ui.text(x, row(8) + 10.0 * s, "YOUR VOICE ON AND KEEPS NOTHING: NOTHING IS", ui::DIM);
    ui.text(x, row(8) + 20.0 * s, "RECORDED OR STORED.", ui::DIM);
    asks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meter_gives_quiet_speech_room() {
        assert_eq!(meter_pos(0.0), 0.0);
        assert_eq!(meter_pos(-1.0), 0.0);
        assert_eq!(meter_pos(5.0), 1.0);
        assert!(meter_pos(0.01) > 0.15);
        assert!(meter_pos(0.01) < meter_pos(0.05));
        // Decibels: full scale is the end, and every doubling is the same step.
        assert!((meter_pos(1.0) - 1.0).abs() < 0.01);
        let (a, b, c) = (meter_pos(0.01), meter_pos(0.02), meter_pos(0.04));
        assert!(((b - a) - (c - b)).abs() < 0.01, "{a} {b} {c}");
        assert!(((b - a) - 6.0206 / 54.0).abs() < 0.01, "a doubling is 6 dB");
        // Speech (about 0.05) lands in the middle of the bar, not at its end.
        assert!(meter_pos(0.05) > 0.45 && meter_pos(0.05) < 0.8);
        for x in [0.003f32, 0.1, 0.7, 12.0, 0.0004] {
            let want = 6.0206 * (x as f64).log2() as f32;
            let got = 6.0206 * log2_guess(x);
            assert!((want - got).abs() < 1.5, "{x}: {want} {got}");
        }
        assert_eq!(meter_pos(f32::NAN), 0.0);
        assert_eq!(meter_pos(f32::INFINITY), 1.0);
        // The loudest threshold is still on the bar.
        assert!(meter_pos(codec::threshold(10)) < 1.0);
    }
}
