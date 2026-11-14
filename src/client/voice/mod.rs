//! Proximity voice chat, the client's half. The server does the work of
//! deciding who hears whom (`net/voice.rs`); this side only listens to
//! the microphone, packs speech into small frames and plays the frames that
//! arrive.
//!
//! - `capture`: the microphone through SDL, into a lock-free `ring`.
//! - `codec`: IMA ADPCM, 16 kHz, 20 ms frames; the loudness and the gate.
//! - `playback` and `stream`: decoding, resampling and the jitter buffer in
//!   the mixer (`client/audio.rs`).
//! - `hud` and `menu`: the icons and SETTINGS > VOICE.
//!
//! Everything is off by default: the microphone is not opened unless the
//! player turned it on in the settings and is on a server that allows it
//! (and, with push to talk, only while the key is held, plus a moment
//! after). Nothing is recorded or stored anywhere.

pub mod capture;
pub mod codec;
pub mod hud;
pub mod menu;
pub mod playback;
pub mod ring;
pub mod stream;

use crate::audio::Audio;
use crate::netplay::Client;
use crate::settings::Settings;
use alloc::string::String;
use alloc::vec::Vec;

/// Push to talk goes on sending this long after the key is let go, so the
/// end of a word is not cut off.
const PTT_TAIL: f32 = 0.25;
/// And the microphone stays open this long, so a second press does not wait
/// for it to open again.
const PTT_KEEP_OPEN: f32 = 1.5;
/// How long a player stays marked as talking after their last frame.
const TALKING_FOR: f32 = 0.4;
/// Seconds before trying to open a microphone that would not open.
const RETRY: f32 = 3.0;
/// Most captured samples held waiting for the game thread (half a second);
/// beyond that the oldest are dropped, so a stalled game never builds delay.
const MAX_PENDING: usize = codec::RATE as usize / 2;

/// What the game gives voice chat each frame while on a server.
pub struct Wire<'a> {
    pub client: &'a mut Client,
    /// Where the player is and which way they face (for panning voices).
    pub pos: [f64; 3],
    pub yaw: f32,
    /// The server lets this player talk (`PERM_VOICE`).
    pub allowed: bool,
}

pub struct Voice {
    capture: Option<Capture>,
    /// The microphone (by name) the open one was opened as.
    opened_as: String,
    encoder: codec::Encoder,
    gate: codec::Gate,
    playback: playback::Playback,
    /// Samples captured, waiting for a whole frame.
    pending: Vec<i16>,
    frame: Vec<u8>,
    seq: u16,
    /// What the server was last told (`VOICE_` flags).
    sent_flags: Option<u8>,
    /// Seconds push to talk still counts as held, and the microphone stays open.
    ptt_tail: f32,
    keep_open: f32,
    /// Seconds until an open that failed is tried again.
    retry: f32,
    /// Seconds our voice was last being sent (for the icon).
    live_for: f32,
    /// Players who are talking, and seconds left on each.
    talking: Vec<(u16, f32)>,
    /// How loud the microphone is now, for the meter (0..1, quick to rise, slow to fall).
    pub level: f32,
    /// The microphone would not open (last time tried).
    pub failed: bool,
    /// The settings screen's MICROPHONE TEST is on.
    pub testing: bool,
    /// Microphones found, and when (UI clock) the list was made.
    pub devices: Vec<String>,
    pub devices_at: f32,
    /// FIND MY MICROPHONE: every microphone listened to at once while the player speaks.
    detect: Option<Detect>,
    /// What it found (the device's name, empty for none) until the settings screen takes it.
    detected: Option<String>,
    /// What the last search said, for the settings screen.
    pub detect_note: String,
}

/// Seconds the search listens.
pub const DETECT_SECONDS: f32 = 4.0;
/// The quietest peak (see `codec::level`) that counts as someone speaking into a microphone.
const DETECT_FLOOR: f32 = 0.006;

struct Detect {
    left: f32,
    /// Each microphone, open, with the loudest moment it has heard.
    mics: Vec<(String, capture::Capture, f32)>,
}

use capture::Capture;

impl Default for Voice {
    fn default() -> Self {
        Voice::new()
    }
}

impl Voice {
    pub fn new() -> Voice {
        Voice {
            capture: None,
            opened_as: String::new(),
            encoder: codec::Encoder::default(),
            gate: codec::Gate::default(),
            playback: playback::Playback::default(),
            pending: Vec::new(),
            frame: Vec::new(),
            seq: 0,
            sent_flags: None,
            ptt_tail: 0.0,
            keep_open: 0.0,
            retry: 0.0,
            live_for: 0.0,
            talking: Vec::new(),
            level: 0.0,
            failed: false,
            testing: false,
            devices: Vec::new(),
            devices_at: -1000.0,
            detect: None,
            detected: None,
            detect_note: String::new(),
        }
    }

    /// Look for microphones again (`now` is the UI clock).
    pub fn refresh_devices(&mut self, now: f32) {
        self.devices = capture::devices();
        self.devices_at = now;
    }

    /// Start listening to every microphone at once: the one that hears the player speak is the
    /// right one. Needs the list of devices (`refresh_devices`); at most 8 are tried.
    pub fn start_detect(&mut self) {
        self.capture = None;
        self.testing = false;
        let mut mics = Vec::new();
        for name in self.devices.iter().take(8) {
            if let Some(c) = capture::Capture::open(name) {
                mics.push((name.clone(), c, 0.0f32));
            }
        }
        self.detected = None;
        self.detect = if mics.is_empty() { None } else { Some(Detect { left: DETECT_SECONDS, mics }) };
        if self.detect.is_none() {
            self.detected = Some(String::new());
        }
    }

    /// Seconds the search has left (0 when none is running).
    pub fn detect_left(&self) -> f32 {
        self.detect.as_ref().map_or(0.0, |d| d.left)
    }

    /// The result of a finished search: `Some(name)`, or `Some("")` when nothing was heard.
    pub fn take_detected(&mut self) -> Option<String> {
        self.detected.take()
    }

    /// Listen for a frame: update each microphone's loudest moment, and finish when the time is up.
    fn step_detect(&mut self, dt: f32) {
        let Some(d) = self.detect.as_mut() else { return };
        d.left -= dt;
        let mut buf = [0i16; 1024];
        for (_, cap, peak) in d.mics.iter_mut() {
            loop {
                let n = cap.read(&mut buf);
                if n == 0 {
                    break;
                }
                for chunk in buf[..n].chunks(codec::FRAME_SAMPLES) {
                    *peak = peak.max(codec::level(chunk));
                }
                if n < buf.len() {
                    break;
                }
            }
        }
        if d.left <= 0.0 {
            let best = d.mics.iter().max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(core::cmp::Ordering::Equal)).map(|m| (m.0.clone(), m.2));
            self.detected = Some(match best {
                Some((name, peak)) if peak >= DETECT_FLOOR => name,
                _ => String::new(),
            });
            self.detect = None;
        }
    }

    /// The microphone is open (and so listening).
    pub fn mic_open(&self) -> bool {
        self.capture.is_some()
    }

    /// What the microphone icon should show.
    pub fn mic(&self) -> hud::Mic {
        if self.live_for > 0.0 {
            hud::Mic::Live
        } else if self.capture.is_some() && !self.testing && self.sent_flags.map_or(false, |f| f & crate::proto::VOICE_TALK != 0) {
            hud::Mic::Armed
        } else {
            hud::Mic::Off
        }
    }

    /// Who is talking now.
    pub fn talking(&self) -> impl Iterator<Item = u16> + '_ {
        self.talking.iter().map(|t| t.0)
    }

    fn mark_talking(&mut self, id: u16) {
        match self.talking.iter_mut().find(|t| t.0 == id) {
            Some(t) => t.1 = TALKING_FOR,
            None => self.talking.push((id, TALKING_FOR)),
        }
    }

    /// Everything stops: not on a server, or no longer allowed.
    fn stop(&mut self) {
        self.capture = None;
        self.pending.clear();
        self.gate.close();
        self.level = 0.0;
        self.live_for = 0.0;
        self.talking.clear();
        self.playback.clear();
    }

    /// Once a frame. `ptt` says the push-to-talk key is held. `wire` is the
    /// server connection (None in single player and in menus); `screen_meter`
    /// is true while the settings screen shows its VOICE tab.
    pub fn update(&mut self, dt: f32, set: &Settings, ptt: bool, mut wire: Option<Wire>, audio: &mut Audio, screen_meter: bool) {
        let dt = dt.clamp(0.0, 0.5);
        if !screen_meter {
            self.testing = false;
        }
        let online = wire.is_some();
        let may_talk = set.voice_mic && wire.as_ref().map_or(false, |w| w.allowed);
        let hear = set.voice_hear && online;

        // Tell the server what we want (once, and again when it changes).
        match wire.as_mut() {
            Some(w) => {
                let want_hear = if hear { crate::proto::VOICE_HEAR } else { 0 };
                let want_talk = if may_talk { crate::proto::VOICE_TALK } else { 0 };
                let flags = want_hear | want_talk;
                if self.sent_flags != Some(flags) {
                    w.client.send_voice_state(flags);
                    self.sent_flags = Some(flags);
                }
            }
            None => {
                if self.sent_flags.take().is_some() || !self.talking.is_empty() {
                    self.stop();
                }
            }
        }

        // Push to talk: the key, and its tail.
        if ptt {
            self.ptt_tail = PTT_TAIL;
            self.keep_open = PTT_KEEP_OPEN;
        } else {
            self.ptt_tail = (self.ptt_tail - dt).max(0.0);
            self.keep_open = (self.keep_open - dt).max(0.0);
        }
        self.step_detect(dt);
        self.live_for = (self.live_for - dt).max(0.0);
        self.retry = (self.retry - dt).max(0.0);

        // The microphone is open only when it is needed.
        let sending = may_talk && (set.voice_activation || self.keep_open > 0.0);
        let listen = sending || (self.testing && screen_meter);
        if !listen {
            if self.capture.is_some() {
                self.capture = None;
            }
            self.pending.clear();
            self.gate.close();
            self.level = 0.0;
            self.failed = false;
        } else {
            if self.capture.is_some() && self.opened_as != set.voice_device {
                self.capture = None; // another microphone was chosen
                self.pending.clear();
            }
            if self.capture.is_none() && self.retry <= 0.0 {
                self.capture = Capture::open(&set.voice_device);
                self.failed = self.capture.is_none();
                if self.failed {
                    self.retry = RETRY;
                } else {
                    self.opened_as = set.voice_device.clone();
                    self.pending.clear();
                }
            }
        }

        // Listen: whole frames out of what the microphone gave.
        if let Some(cap) = self.capture.as_ref() {
            let mut buf = [0i16; 1024];
            loop {
                let n = cap.read(&mut buf);
                self.pending.extend_from_slice(&buf[..n]);
                if n < buf.len() {
                    break;
                }
            }
            if self.pending.len() > MAX_PENDING {
                let excess = self.pending.len() - MAX_PENDING;
                self.pending.drain(..excess);
            }
        }
        let activation = codec::threshold(set.voice_threshold);
        let mut frame = [0i16; codec::FRAME_SAMPLES];
        while self.capture.is_some() && self.pending.len() >= codec::FRAME_SAMPLES {
            frame.copy_from_slice(&self.pending[..codec::FRAME_SAMPLES]);
            self.pending.drain(..codec::FRAME_SAMPLES);
            let loud = codec::level(&frame);
            self.level = if loud > self.level { loud } else { self.level * 0.9 };
            if !sending {
                continue; // only the meter wanted this
            }
            let open = if set.voice_activation { self.gate.update(loud, activation, 0.02) } else { self.ptt_tail > 0.0 };
            if !open {
                continue;
            }
            if let Some(w) = wire.as_mut() {
                self.frame.clear();
                self.encoder.encode(&frame, &mut self.frame);
                w.client.send_voice(self.seq, &self.frame);
                self.seq = self.seq.wrapping_add(1);
                self.live_for = 0.3;
            }
        }

        // Others: play what the server passed on.
        match wire.as_mut() {
            Some(w) => {
                let heard: Vec<(u16, u16, u8, Vec<u8>)> = w.client.voice_in.drain(..).collect();
                // 8 is the middle of the slider: the voice as it came.
                let volume = set.voice_volume as f32 / 8.0 * set.master_volume as f32 / 10.0;
                for (id, seq, loud, data) in heard {
                    if !hear {
                        continue;
                    }
                    let pan = match w.client.players.get(&id) {
                        Some(p) if p.seen => playback::pan_toward(w.yaw, w.pos, p.pos),
                        _ => 0.0,
                    };
                    if self.playback.play(audio, id, seq, loud, volume, pan, &data) {
                        self.mark_talking(id);
                    }
                }
                // Players who left are not talking.
                let players = &w.client.players;
                self.talking.retain(|t| players.contains_key(&t.0));
            }
            None => {}
        }
        for t in self.talking.iter_mut() {
            t.1 -= dt;
        }
        self.talking.retain(|t| t.1 > 0.0);
    }
}
