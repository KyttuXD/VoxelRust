//! What the session keeps for the host functions of this folder: the
//! pictures scripts ask for this frame, the physics bodies, the storages.
//! It lives in `Session::ext` under [`KEY`], made on first use.

use crate::inventory::Inventory;
use crate::player::Player;
use crate::session::{Controls, Session};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// The key in `Session::ext`.
pub const KEY: &str = "script-ext";
/// The name `Session::opened_ext` carries for a script's storage.
pub const STORAGE_KEY: &str = "script-storage";

/// Most models, specks, rectangles and texts a frame may carry.
pub const MAX_MODELS: usize = 200;
pub const MAX_SPECKS: usize = 300;
pub const MAX_RECTS: usize = 64;
pub const MAX_TEXTS: usize = 32;
pub const MAX_PICS: usize = 8;
/// Most bodies and storages, over all mods.
pub const MAX_BODIES: usize = 256;
pub const MAX_STORAGES: usize = 128;

/// A model to draw in the world this frame.
pub struct DrawModel {
    /// Index into the registry's models.
    pub model: usize,
    pub at: [f64; 3],
    pub yaw: f32,
    pub height: f32,
    pub phase: f32,
    /// 0..1 for a fixed brightness, or below 0 for the light of the place.
    pub tint: f32,
    /// Drawn only when the camera is outside the thing (a vehicle the player sits in).
    pub third_only: bool,
    /// How the model is posed: stride 0..1, where the head looks (degrees), seconds alive.
    pub stride: f32,
    pub head_pitch: f32,
    pub head_yaw: f32,
    pub clock: f32,
}

/// A rectangle of the HUD, as fractions of the screen.
pub struct HudRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub col: [u8; 4],
}

/// A mod's picture asked for with `hud_frac_image`, to be drawn on the HUD: a place and size as
/// fractions of the screen.
pub struct HudPic {
    pub owner: String,
    pub dir: String,
    pub file: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub alpha: f32,
}

/// A line of text on the HUD; the position is a fraction of the screen.
pub struct HudText {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub text: String,
    pub col: [u8; 4],
}

/// A chest-sized storage a mod keeps (see `storage.rs`). An empty `key` is a
/// free place.
pub struct Storage {
    pub key: String,
    pub inv: Inventory,
}

#[derive(Default)]
pub struct ScriptData {
    /// What the player asked for this frame (set before `on_update` and `on_interact`).
    pub controls: Controls,
    pub models: Vec<DrawModel>,
    pub specks: Vec<([f64; 3], [u8; 3])>,
    pub rects: Vec<HudRect>,
    pub texts: Vec<HudText>,
    pub pics: Vec<HudPic>,
    /// A colour over the whole screen this frame (`hud_tint`).
    pub tint: Option<[u8; 4]>,
    /// Physics bodies: their number and the body.
    pub bodies: Vec<(u32, Player)>,
    pub next_body: u32,
    pub storages: Vec<Storage>,
    /// Messages sent this frame (`net_send` allows a few).
    pub net_sent: u32,
    /// How many messages this player may still send (`send_message`: 8 a second).
    pub msg_bucket: crate::net::modmsg::Bucket,
    /// `set_bone` calls made this frame (at most [`MAX_BONE_CALLS`]).
    pub bone_calls: u32,
    /// A screen a script asked to open (`open_crafting`, `open_inventory`).
    pub screen: Option<super::screens::ScreenRequest>,
}

/// Most `set_bone` calls a frame, over all mods.
pub const MAX_BONE_CALLS: u32 = 4096;

/// The session's script data, made if there is none yet.
pub fn data(g: &mut Session) -> &mut ScriptData {
    let at = match g.ext.iter().position(|e| e.0 == KEY) {
        Some(i) => i,
        None => {
            g.ext.push((KEY, Box::new(ScriptData::default())));
            g.ext.len() - 1
        }
    };
    // The entry is ours (put under our key); a foreign value there is replaced.
    if g.ext[at].1.downcast_ref::<ScriptData>().is_none() {
        g.ext[at].1 = Box::new(ScriptData::default());
    }
    match g.ext[at].1.downcast_mut::<ScriptData>() {
        Some(d) => d,
        None => unreachable!("the entry was just made a ScriptData"),
    }
}

/// Forget what was drawn last frame (called once a frame before the scripts run).
pub fn new_frame(g: &mut Session) {
    if g.ext.iter().all(|e| e.0 != KEY) {
        return;
    }
    let d = data(g);
    d.models.clear();
    d.specks.clear();
    d.rects.clear();
    d.pics.clear();
    d.texts.clear();
    d.tint = None;
    d.net_sent = 0;
    d.bone_calls = 0;
}
