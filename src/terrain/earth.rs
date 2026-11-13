//! Real-world terrain.
//!
//! Heights come from the public "terrarium" elevation tiles, and land cover
//! (water, forest, sand, ice...) from the colours of OpenStreetMap's standard
//! map tiles, which use the same tiling. Both are fetched on demand and kept
//! on disk. A world is a flat piece of the Web Mercator map: block
//! coordinates are a scaled, shifted copy of tile pixel coordinates, so no
//! trigonometry is needed once a world's origin is fixed.
//!
//! Map data (c) OpenStreetMap contributors. Elevation tiles: Mapzen / AWS
//! Terrain Tiles.

use crate::chunk::CH;
use crate::png;
use crate::sys;
use crate::worldgen::{Biome, Column, WorldKind, SEA};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

/// Where a new Earth world starts unless the player picks somewhere else.
pub fn default_kind() -> WorldKind {
    let p = &PLACES[0];
    WorldKind::Earth { lat: p.1, lon: p.2, scale: 30, vscale: 75, features: true }
}

/// Starting places offered when creating an Earth world.
pub const PLACES: [(&str, f64, f64); 10] = [
    ("MOUNT RAINIER", 46.8523, -121.7603),
    ("GRAND CANYON", 36.1069, -112.1129),
    ("MOUNT FUJI", 35.3606, 138.7274),
    ("MATTERHORN", 45.9763, 7.6586),
    ("MANHATTAN", 40.7831, -73.9712),
    ("LONDON", 51.5072, -0.1276),
    ("SYDNEY HARBOUR", -33.8568, 151.2153),
    ("MOUNT EVEREST", 27.9881, 86.9250),
    ("NILE DELTA", 30.9, 31.1),
    ("HAWAII", 19.6, -155.5),
];

// ------------------------------------------------------------ small maths
// (f64 versions of the few functions the map projection needs; `core` has
// none of them.)

const PI: f64 = core::f64::consts::PI;

fn sin64(x: f64) -> f64 {
    // Reduce to [-pi, pi], fold to [-pi/2, pi/2], then a Taylor series.
    let k = crate::math::floord(x / (2.0 * PI) + 0.5) as f64;
    let mut r = x - k * 2.0 * PI;
    if r > PI / 2.0 {
        r = PI - r;
    } else if r < -PI / 2.0 {
        r = -PI - r;
    }
    let r2 = r * r;
    let (mut term, mut sum) = (r, r);
    for n in 1..10 {
        term *= -r2 / ((2 * n) as f64 * (2 * n + 1) as f64);
        sum += term;
    }
    sum
}

fn cos64(x: f64) -> f64 {
    sin64(x + PI / 2.0)
}

fn ln64(x: f64) -> f64 {
    if x <= 0.0 {
        return -1e300;
    }
    // x = m * 2^k with m in [1, 2); ln(m) by the atanh series.
    let bits = x.to_bits();
    let k = ((bits >> 52) & 0x7FF) as i64 - 1023;
    let m = f64::from_bits((bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000);
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let (mut term, mut sum) = (t, t);
    for n in 1..30 {
        term *= t2;
        sum += term / (2 * n + 1) as f64;
    }
    2.0 * sum + k as f64 * core::f64::consts::LN_2
}

fn exp64(x: f64) -> f64 {
    // exp(x) = exp(x / 16)^16, with a short series for the small part.
    let y = x / 16.0;
    let (mut term, mut sum) = (1.0, 1.0);
    for n in 1..16 {
        term *= y / n as f64;
        sum += term;
    }
    let mut r = sum;
    for _ in 0..4 {
        r *= r;
    }
    r
}

/// Web Mercator: latitude in degrees to 0 (north edge) .. 1 (south edge).
pub fn lat_to_unit(lat: f64) -> f64 {
    let phi = lat.clamp(-85.05, 85.05) * PI / 180.0;
    let (s, c) = (sin64(phi), cos64(phi));
    0.5 - ln64((1.0 + s) / c) / (2.0 * PI)
}

/// The inverse of [`lat_to_unit`], good to about a hundredth of a degree
/// (it is only used to show coordinates).
pub fn unit_to_lat(y: f64) -> f64 {
    let n = PI * (1.0 - 2.0 * y);
    let sinh = (exp64(n) - exp64(-n)) * 0.5;
    crate::math::atan2f(sinh as f32, 1.0) as f64 * 180.0 / PI
}

// ---------------------------------------------------------------- projection

/// How block coordinates map onto the tiled world map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Proj {
    /// Zoom level of the tiles terrain is built from.
    pub zoom: u32,
    /// Pixel position of block (0, 0) at that zoom.
    pub px0: f64,
    pub py0: f64,
    /// Blocks per tile pixel.
    pub k: f64,
    pub lat0: f64,
    /// Metres per block, up and down.
    pub vscale: f64,
    /// Metres per block across.
    pub scale: f64,
    /// Roads and buildings from the map are built.
    pub features: bool,
}

/// Elevation tiles stop at this zoom; finer worlds interpolate between
/// their pixels.
const MAX_ELEVATION_ZOOM: u32 = 14;

/// The elevation (metres) that sits at sea level in a finely scaled world,
/// found from the world's origin the first time it is needed. `i32::MIN`
/// means "not worked out yet".
static BASE_M: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(i32::MIN);

/// Forget the base elevation (a different world is being opened).
pub fn reset_base() {
    BASE_M.store(i32::MIN, Ordering::Release);
}

impl Proj {
    pub fn new(lat: f64, lon: f64, scale: u32, vscale: u32) -> Proj {
        let cos = cos64(lat * PI / 180.0).max(0.05);
        // Metres per pixel at zoom z is 156543.03 * cos(lat) / 2^z. Pick the
        // deepest zoom whose pixels are no smaller than a block.
        let mut zoom = 2u32;
        while zoom < 17 && 156_543.033_92 * cos / (1u64 << (zoom + 1)) as f64 >= scale as f64 {
            zoom += 1;
        }
        let size = (256u64 << zoom) as f64;
        let k = 156_543.033_92 * cos / (1u64 << zoom) as f64 / scale as f64;
        Proj { zoom, px0: (lon + 180.0) / 360.0 * size, py0: lat_to_unit(lat) * size, k, lat0: lat, vscale: vscale as f64, scale: scale as f64, features: true }
    }

    /// Tile pixel coordinates (at `self.zoom`) of a block position.
    pub fn block_to_px(&self, x: f64, z: f64) -> (f64, f64) {
        (self.px0 + x / self.k, self.py0 + z / self.k)
    }

    pub fn px_to_block(&self, px: f64, py: f64) -> (f64, f64) {
        ((px - self.px0) * self.k, (py - self.py0) * self.k)
    }

    /// Latitude and longitude (degrees) of a block position.
    pub fn block_to_latlon(&self, x: f64, z: f64) -> (f64, f64) {
        let size = (256u64 << self.zoom) as f64;
        let (px, py) = self.block_to_px(x, z);
        (unit_to_lat(py / size), px / size * 360.0 - 180.0)
    }

    pub fn latlon_to_block(&self, lat: f64, lon: f64) -> (f64, f64) {
        let size = (256u64 << self.zoom) as f64;
        self.px_to_block((lon + 180.0) / 360.0 * size, lat_to_unit(lat) * size)
    }
}

// --------------------------------------------------------------------- tiles

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Layer {
    Elevation,
    Map,
}

/// Land cover classes read off the map tiles.
pub mod class {
    pub const LAND: u8 = 0;
    pub const WATER: u8 = 1;
    pub const FOREST: u8 = 2;
    pub const GRASS: u8 = 3;
    pub const SAND: u8 = 4;
    pub const ICE: u8 = 5;
    pub const ROCK: u8 = 6;
    pub const URBAN: u8 = 7;
    pub const BUILDING: u8 = 8;
    /// A road of any size (the lines the map draws).
    pub const ROAD: u8 = 9;
    /// Farmland, farmyards, orchards and vineyards.
    pub const FARM: u8 = 10;
}

pub struct Tile {
    /// Elevation tiles: metres. Map tiles: empty.
    pub metres: Vec<i16>,
    /// Map tiles: a land cover class per pixel. Elevation tiles: empty.
    pub cover: Vec<u8>,
    /// Map tiles: the picture itself, for the map screen.
    pub rgba: Vec<u8>,
}

pub type TileKey = (Layer, u32, u32, u32);

/// What colour means what on OpenStreetMap's standard map.
fn classify(r: u8, g: u8, b: u8) -> u8 {
    const KNOWN: [([u8; 3], u8); 20] = [
        ([170, 211, 223], class::WATER),
        ([173, 209, 158], class::FOREST),
        ([200, 215, 171], class::FOREST),
        ([205, 235, 176], class::GRASS),
        ([200, 250, 204], class::GRASS),
        ([238, 240, 213], class::FARM),
        ([245, 220, 186], class::FARM),
        ([174, 223, 163], class::FARM),
        ([222, 252, 225], class::GRASS),
        ([245, 233, 198], class::SAND),
        ([255, 241, 186], class::SAND),
        ([221, 236, 236], class::ICE),
        ([238, 229, 220], class::ROCK),
        ([224, 223, 223], class::URBAN),
        ([217, 208, 201], class::BUILDING),
        ([255, 255, 255], class::ROAD),
        ([247, 250, 191], class::ROAD),
        ([252, 214, 164], class::ROAD),
        ([249, 178, 156], class::ROAD),
        ([232, 146, 162], class::ROAD),
    ];
    let mut best = (40 * 40 * 3, class::LAND);
    for (c, k) in KNOWN {
        let d = [(r, c[0]), (g, c[1]), (b, c[2])].iter().map(|&(a, b)| (a as i32 - b as i32) * (a as i32 - b as i32)).sum::<i32>();
        if d < best.0 {
            best = (d, k);
        }
    }
    // Only accept a close match: labels and odd features stay plain land.
    if best.0 <= 3 * 9 * 9 {
        best.1
    } else {
        class::LAND
    }
}

/// Decode a downloaded tile. `None` if it is not a 256 x 256 PNG.
pub fn decode_tile(layer: Layer, bytes: &[u8]) -> Option<Tile> {
    let img = png::decode(bytes)?;
    if img.w != 256 || img.h != 256 {
        return None;
    }
    let px = |i: usize| (img.rgba[i * 4], img.rgba[i * 4 + 1], img.rgba[i * 4 + 2]);
    Some(match layer {
        Layer::Elevation => Tile {
            // metres = R * 256 + G + B / 256 - 32768
            metres: (0..65536).map(|i| px(i)).map(|(r, g, _)| (r as i32 * 256 + g as i32 - 32768).clamp(-32000, 32000) as i16).collect(),
            cover: Vec::new(),
            rgba: Vec::new(),
        },
        Layer::Map => Tile { metres: Vec::new(), cover: (0..65536).map(|i| px(i)).map(|(r, g, b)| classify(r, g, b)).collect(), rgba: img.rgba },
    })
}

enum Slot {
    Loading,
    Ready(Arc<Tile>),
    Failed,
}

/// A tiny lock that works without SDL (tests have none).
struct Spin(AtomicBool);

impl Spin {
    fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        while self.0.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        let r = f();
        self.0.store(false, Ordering::Release);
        r
    }
}

static LOCK: Spin = Spin(AtomicBool::new(false));
static mut TILES: Option<BTreeMap<TileKey, Slot>> = None;
static CONF: AtomicPtr<Conf> = AtomicPtr::new(core::ptr::null_mut());

struct Conf {
    /// Folder (with trailing separator) for downloaded tiles.
    cache: String,
    /// `host:port` of a VoxelRust server that fetches tiles for us, or empty.
    relay: String,
}

/// Say where tiles are cached and how they are fetched. Until this is
/// called nothing is downloaded (tiles can still be put in by hand).
pub fn configure(cache_dir: &str, relay: &str) {
    let c = alloc::boxed::Box::new(Conf { cache: String::from(cache_dir), relay: String::from(relay) });
    // The old value is leaked on purpose: other threads may be reading it.
    CONF.store(alloc::boxed::Box::into_raw(c), Ordering::Release);
}

fn tiles<R>(f: impl FnOnce(&mut BTreeMap<TileKey, Slot>) -> R) -> R {
    // SAFETY: TILES is only ever touched inside LOCK.
    LOCK.with(|| unsafe {
        let map = &mut *core::ptr::addr_of_mut!(TILES);
        f(map.get_or_insert_with(BTreeMap::new))
    })
}

/// Put a tile in the memory cache directly (tests, and tiles relayed by a
/// multiplayer server).
pub fn insert_tile(key: TileKey, tile: Tile) {
    tiles(|m| {
        m.insert(key, Slot::Ready(Arc::new(tile)));
    });
}

pub fn tile_url(key: TileKey) -> String {
    match key.0 {
        Layer::Elevation => alloc::format!("https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{}/{}/{}.png", key.1, key.2, key.3),
        Layer::Map => alloc::format!("https://tile.openstreetmap.org/{}/{}/{}.png", key.1, key.2, key.3),
    }
}

fn cache_path(conf: &Conf, key: TileKey) -> String {
    alloc::format!("{}tile-{}-{}-{}-{}.png", conf.cache, if key.0 == Layer::Elevation { "elev" } else { "map" }, key.1, key.2, key.3)
}

/// Download one tile to `path` with the system's `curl`. The command line
/// is built only from numbers and our own cache path.
pub fn download_with_curl(key: TileKey, path: &str) -> bool {
    if path.contains('"') || path.contains('$') || path.contains('`') || path.contains('%') {
        return false; // never hand the shell a path that could be misread
    }
    let cmd = alloc::format!("curl -fsSL --max-time 25 -A \"VoxelRust/{}\" -o \"{}\" \"{}\"", env!("CARGO_PKG_VERSION"), path, tile_url(key));
    sys::run_command(&cmd) == 0
}

/// Ask a VoxelRust server for a tile: one line out, length-prefixed bytes back.
fn download_via_relay(relay: &str, key: TileKey) -> Option<Vec<u8>> {
    use crate::sys::net::TcpStream;
    let (host, port) = relay.rsplit_once(':')?;
    let mut s = TcpStream::connect(host, port.parse().ok()?, 8000).ok()?;
    let line = alloc::format!("TILE {} {} {} {}\n", if key.0 == Layer::Elevation { "elev" } else { "map" }, key.1, key.2, key.3);
    s.send_all(line.as_bytes(), 8000).ok()?;
    let mut len = [0u8; 4];
    s.recv_exact(&mut len, 40_000).ok()?;
    let n = u32::from_le_bytes(len) as usize;
    if n == 0 || n > 4 << 20 {
        return None;
    }
    let mut data = alloc::vec![0u8; n];
    s.recv_exact(&mut data, 40_000).ok()?;
    Some(data)
}

/// Get a tile's bytes: from disk if cached, else from the network.
pub fn fetch_bytes(key: TileKey) -> Option<Vec<u8>> {
    let conf = unsafe { CONF.load(Ordering::Acquire).as_ref()? };
    let path = cache_path(conf, key);
    if let Some(bytes) = sys::read_file(&path) {
        if !bytes.is_empty() {
            return Some(bytes);
        }
    }
    if !conf.relay.is_empty() {
        let bytes = download_via_relay(&conf.relay, key)?;
        sys::write_file(&path, &bytes);
        return Some(bytes);
    }
    if download_with_curl(key, &path) {
        sys::read_file(&path)
    } else {
        None
    }
}

/// A tile, loading it if need be (this can take seconds the first time).
/// Safe to call from several threads: only one fetches, the others wait.
pub fn tile(key: TileKey) -> Option<Arc<Tile>> {
    let limit = 1u32 << key.1.min(20);
    if key.2 >= limit || key.3 >= limit {
        return None;
    }
    for _ in 0..4000 {
        enum Next {
            Got(Option<Arc<Tile>>),
            Fetch,
            Wait,
        }
        let next = tiles(|m| match m.get(&key) {
            Some(Slot::Ready(t)) => Next::Got(Some(t.clone())),
            Some(Slot::Failed) => Next::Got(None),
            Some(Slot::Loading) => Next::Wait,
            None => {
                // Keep memory bounded: forget everything now and then.
                if m.len() > 160 {
                    m.retain(|_, s| matches!(s, Slot::Loading));
                }
                m.insert(key, Slot::Loading);
                Next::Fetch
            }
        });
        match next {
            Next::Got(t) => return t,
            Next::Wait => sys::sleep_ms(10),
            Next::Fetch => {
                let loaded = fetch_bytes(key).and_then(|b| decode_tile(key.0, &b)).map(Arc::new);
                tiles(|m| {
                    m.insert(key, loaded.clone().map_or(Slot::Failed, Slot::Ready));
                });
                return loaded;
            }
        }
    }
    None
}

// ---------------------------------------------------------------- place names

/// Pick the name for a detail level out of a reverse-geocoding answer:
/// 1 country, 2 region (state, province...), 3 town.
pub fn place_from_json(text: &str, level: i32) -> Option<String> {
    let json = crate::json::parse(text).ok()?;
    let addr = json.get("address")?;
    let pick = |keys: &[&str]| keys.iter().find_map(|k| addr.get(k).and_then(|v| v.as_str())).map(String::from);
    let country = pick(&["country"]);
    let region = pick(&["state", "region", "province", "state_district", "county"]);
    let town = pick(&["city", "town", "village", "municipality", "suburb", "hamlet"]);
    let name = match level {
        1 => country,
        2 => region.or(country),
        _ => town.or(region).or(country),
    }?;
    // The built-in font is ASCII: drop what it cannot draw.
    let clean: String = name.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).take(40).collect();
    if clean.trim().is_empty() {
        None
    } else {
        Some(clean)
    }
}

/// Ask OpenStreetMap's Nominatim service what is at a position. Returns
/// the raw JSON answer. Used sparingly: one question every several seconds.
pub fn fetch_place_json(lat: f64, lon: f64, level: i32) -> Option<Vec<u8>> {
    let conf = unsafe { CONF.load(Ordering::Acquire).as_ref()? };
    let zoom = match level {
        1 => 3,
        2 => 5,
        _ => 10,
    };
    if !conf.relay.is_empty() {
        use crate::sys::net::TcpStream;
        let (host, port) = conf.relay.rsplit_once(':')?;
        let mut s = TcpStream::connect(host, port.parse().ok()?, 8000).ok()?;
        s.send_all(alloc::format!("GEO {:.5} {:.5} {}\n", lat, lon, level).as_bytes(), 8000).ok()?;
        let mut len = [0u8; 4];
        s.recv_exact(&mut len, 30_000).ok()?;
        let n = u32::from_le_bytes(len) as usize;
        if n == 0 || n > 1 << 16 {
            return None;
        }
        let mut data = alloc::vec![0u8; n];
        s.recv_exact(&mut data, 30_000).ok()?;
        return Some(data);
    }
    let path = alloc::format!("{}place.json", conf.cache);
    if path.contains('"') || path.contains('$') || path.contains('`') || path.contains('%') {
        return None;
    }
    // Built only from numbers and our own path.
    let cmd = alloc::format!(
        "curl -fsSL --max-time 15 -A \"VoxelRust/{}\" -o \"{}\" \"https://nominatim.openstreetmap.org/reverse?format=jsonv2&lat={:.5}&lon={:.5}&zoom={}&accept-language=en\"",
        env!("CARGO_PKG_VERSION"),
        path,
        lat,
        lon,
        zoom
    );
    if sys::run_command(&cmd) != 0 {
        return None;
    }
    sys::read_file(&path)
}

/// The name of the place at a position, at the given detail level.
pub fn place_name(lat: f64, lon: f64, level: i32) -> Option<String> {
    let bytes = fetch_place_json(lat, lon, level)?;
    place_from_json(&String::from_utf8_lossy(&bytes), level)
}

/// Forget failed downloads so they are tried again (after going online).
pub fn retry_failed() {
    tiles(|m| m.retain(|_, s| !matches!(s, Slot::Failed)));
}

// ------------------------------------------------------------------- terrain

fn sample_metres(zoom: u32, px: f64, py: f64) -> Option<f32> {
    let size = (256u64 << zoom) as f64;
    let py = py.clamp(0.0, size - 1.001);
    // Wrap around the date line.
    let px = px - crate::math::floord(px / size) as f64 * size;
    let (x0, y0) = (px as u64, py as u64);
    let (fx, fy) = ((px - x0 as f64) as f32, (py - y0 as f64) as f32);
    let mut v = [0f32; 4];
    for (i, (dx, dy)) in [(0, 0), (1, 0), (0, 1), (1, 1)].into_iter().enumerate() {
        let (x, y) = ((x0 + dx) % (256u64 << zoom), (y0 + dy).min((256u64 << zoom) - 1));
        let t = tile((Layer::Elevation, zoom, (x / 256) as u32, (y / 256) as u32))?;
        v[i] = t.metres[((y % 256) * 256 + x % 256) as usize] as f32;
    }
    let (top, bottom) = (v[0] + (v[1] - v[0]) * fx, v[2] + (v[3] - v[2]) * fx);
    Some(top + (bottom - top) * fy)
}

fn sample_cover(zoom: u32, px: f64, py: f64) -> u8 {
    let size = 256u64 << zoom;
    if py < 0.0 || py >= size as f64 {
        return class::LAND;
    }
    let px = px - crate::math::floord(px / size as f64) as f64 * size as f64;
    let (x, y) = ((px as u64).min(size - 1), py as u64);
    match tile((Layer::Map, zoom, (x / 256) as u32, (y / 256) as u32)) {
        Some(t) => t.cover[((y % 256) * 256 + x % 256) as usize],
        None => class::LAND,
    }
}

/// Height in blocks for an elevation in metres.
pub fn metres_to_height(m: f32, vscale: f64) -> i32 {
    let h = if m <= 0.0 { SEA - 1 + (m as f64 / vscale) as i32 } else { SEA + 1 + (m as f64 / vscale) as i32 };
    h.clamp(6, CH - 14)
}

/// What the map shows at block (x, z): one of [`class`]. Much cheaper than
/// a whole [`column`], for looking at the neighbours of a road or a wall.
pub fn cover_at(p: &Proj, x: i32, z: i32) -> u8 {
    let (px, py) = p.block_to_px(x as f64 + 0.5, z as f64 + 0.5);
    sample_cover(p.zoom, px, py)
}

/// The terrain column at block (x, z) of an Earth world.
pub fn column(p: &Proj, x: i32, z: i32) -> Column {
    let (px, py) = p.block_to_px(x as f64 + 0.5, z as f64 + 0.5);
    // Elevation comes from a shallower zoom than the map when the world is
    // finer than the elevation data.
    let ez = p.zoom.min(MAX_ELEVATION_ZOOM);
    let shrink = (1u64 << (p.zoom - ez)) as f64;
    let sample = |px: f64, py: f64| sample_metres(ez, px / shrink, py / shrink);
    let mut m = sample(px, py).unwrap_or(-10.0);
    // With blocks only a few metres tall the world is not high enough to
    // hold real altitudes, so heights are measured from near the origin's.
    if p.vscale < 20.0 {
        let mut base = BASE_M.load(Ordering::Acquire);
        if base == i32::MIN {
            let origin = sample(p.px0, p.py0).unwrap_or(0.0);
            base = (origin - 14.0 * p.vscale as f32).max(0.0) as i32;
            BASE_M.store(base, Ordering::Release);
        }
        if m > 0.0 {
            m = (m - base as f32).max(0.5);
        }
    }
    let mut cover = sample_cover(p.zoom, px, py);
    let mut building = 0u8;
    if cover == class::BUILDING {
        if p.features && m > 0.0 {
            // The map gives outlines, not heights, so height is judged the
            // way an eye would: a broad footprint is a tall building, a
            // small one a house. Measure how far the building runs each way.
            // One block is 1/k map pixels; step at least a pixel at a time.
            let stride = (1.0 / p.k).max(1.0);
            let run = |dx: f64, dy: f64| {
                let mut n = 0;
                while n < 10 && sample_cover(p.zoom, px + dx * (n + 1) as f64 * stride, py + dy * (n + 1) as f64 * stride) == class::BUILDING {
                    n += 1;
                }
                n
            };
            let across = (run(1.0, 0.0) + run(-1.0, 0.0) + 1).min(run(0.0, 1.0) + run(0.0, -1.0) + 1);
            // Metres across its narrow side.
            let breadth = across as f64 * stride * p.k * p.scale;
            // A storey is about three metres; one to two for a house, up
            // to twenty for the broadest blocks, a little varied per lot.
            let lot = crate::noise::hash2(0xB11D, x >> 5, z >> 5);
            let storeys = (1.0 + breadth / 9.0 + (lot % 3) as f64).clamp(1.0, 20.0);
            let metres = storeys * 3.0 + 1.0;
            building = (metres / p.vscale.max(1.0) * (30.0 / p.scale.max(1.0)).clamp(0.25, 1.0)).clamp(3.0, 60.0) as u8;
        }
        cover = class::URBAN;
    }
    let road = cover == class::ROAD && p.features && m > 0.0;
    let farm = cover == class::FARM && m > 0.0;
    if cover == class::ROAD {
        cover = class::URBAN;
    }
    if cover == class::FARM {
        cover = class::LAND;
    }
    if !p.features && cover == class::URBAN {
        cover = class::LAND;
    }
    let height = metres_to_height(m, p.vscale);
    let lat = {
        let l = unit_to_lat(py / (256u64 << p.zoom) as f64);
        if l < 0.0 {
            -l
        } else {
            l
        }
    };
    // Permanent snow starts lower the further from the equator.
    let snow_line = (5200.0 - 75.0 * lat as f32).max(0.0);
    let biome = match cover {
        class::FOREST => {
            if lat > 58.0 || m > snow_line * 0.7 {
                Biome::Taiga
            } else if lat < 14.0 {
                Biome::Jungle
            } else {
                Biome::Forest
            }
        }
        // Open grassland in the tropics is savanna.
        class::GRASS if lat > 4.0 && lat < 22.0 => Biome::Savanna,
        class::SAND => Biome::Desert,
        class::ICE => Biome::Tundra,
        class::ROCK => Biome::Mountain,
        _ => {
            if m > snow_line {
                Biome::Tundra
            } else if m > snow_line * 0.78 {
                Biome::Mountain
            } else if lat > 66.0 {
                Biome::Tundra
            } else {
                Biome::Plains
            }
        }
    };
    Column { height, biome, river: cover == class::WATER && m > 0.0, paved: cover == class::URBAN && m > 0.0, building, road, farm }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELEV: &[u8] = include_bytes!("../../tests/fixtures/terrarium_8_40_97.png");
    const MAP: &[u8] = include_bytes!("../../tests/fixtures/osm_8_40_97.png");

    #[test]
    fn maths_matches_std() {
        for &x in &[-3.0f64, -1.0, -0.2, 0.0, 0.5, 1.5, 3.1, 10.0] {
            assert!((sin64(x) - x.sin()).abs() < 1e-9, "sin {x}");
            assert!((cos64(x) - x.cos()).abs() < 1e-9, "cos {x}");
            assert!((exp64(x.clamp(-3.2, 3.2)) - x.clamp(-3.2, 3.2).exp()).abs() < 1e-6, "exp {x}");
        }
        for &x in &[0.01f64, 0.5, 1.0, 2.7, 40.0] {
            assert!((ln64(x) - x.ln()).abs() < 1e-9, "ln {x}");
        }
    }

    #[test]
    fn mercator_round_trips() {
        assert!((lat_to_unit(0.0) - 0.5).abs() < 1e-9);
        for &lat in &[-60.0f64, -33.86, 0.0, 27.99, 46.85, 70.0] {
            let y = lat_to_unit(lat);
            let want = 0.5 - ((std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.0).tan().ln()) / (2.0 * std::f64::consts::PI);
            assert!((y - want).abs() < 1e-9, "lat {lat}");
            assert!((unit_to_lat(y) - lat).abs() < 0.02, "inverse of {lat} gave {}", unit_to_lat(y));
        }
    }

    #[test]
    fn projection_picks_a_zoom_near_one_block_per_pixel() {
        let p = Proj::new(46.8523, -121.7603, 30, 75);
        assert!(p.k >= 1.0 && p.k < 2.0, "k = {}", p.k);
        assert_eq!(p.zoom, 11);
        // The origin maps to the right tile: Rainier is in tile 331/721 at zoom 11.
        assert_eq!(((p.px0 / 256.0) as u32, (p.py0 / 256.0) as u32), (331, 721));
        // East is +x, south is +z, and the round trip is exact.
        let (lat, lon) = p.block_to_latlon(1000.0, 0.0);
        assert!(lon > -121.7603 && (lat - 46.8523).abs() < 0.02);
        let (lat2, _) = p.block_to_latlon(0.0, 1000.0);
        assert!(lat2 < 46.8523);
        let (x, z) = p.px_to_block(p.block_to_px(123.0, -456.0).0, p.block_to_px(123.0, -456.0).1);
        assert!((x - 123.0).abs() < 1e-6 && (z + 456.0).abs() < 1e-6);
        // 1000 blocks east at 30 m per block is about 30 km: 0.39 degrees here.
        assert!((lon + 121.7603 - 0.394).abs() < 0.01, "lon moved {}", lon + 121.7603);
        // A coarser world uses a shallower zoom; a huge one bottoms out.
        assert!(Proj::new(46.85, -121.76, 1000, 75).zoom < 8);
        // One metre per block: the deepest map zoom, still about a block per pixel.
        let fine = Proj::new(46.85, -121.76, 1, 1);
        assert!(fine.zoom == 16 || fine.zoom == 17, "zoom {}", fine.zoom);
        assert!(fine.k >= 0.8 && fine.k < 2.0, "k = {}", fine.k);
        assert_eq!(Proj::new(0.0, 0.0, 100_000, 75).zoom, 2);
        let (bx, bz) = p.latlon_to_block(46.8523, -121.7603);
        assert!(bx.abs() < 1e-6 && bz.abs() < 1e-6);
    }

    #[test]
    fn place_names_come_out_of_the_geocoder_answer() {
        let answer = r#"{"place_id":1,"name":"Pierce County","display_name":"x","address":{"county":"Pierce County","state":"Washington","country":"United States","country_code":"us"}}"#;
        assert_eq!(place_from_json(answer, 1).as_deref(), Some("United States"));
        assert_eq!(place_from_json(answer, 2).as_deref(), Some("Washington"));
        // No town in the answer: the next thing up is used.
        assert_eq!(place_from_json(answer, 3).as_deref(), Some("Washington"));
        let town = r#"{"address":{"town":"Zermatt","state":"Valais/Wallis","country":"Schweiz/Suisse/Svizzera/Svizra"}}"#;
        assert_eq!(place_from_json(town, 3).as_deref(), Some("Zermatt"));
        // The open sea has no address; junk is not an answer.
        assert_eq!(place_from_json(r#"{"error":"Unable to geocode"}"#, 1), None);
        assert_eq!(place_from_json("not json", 1), None);
        assert_eq!(place_from_json(r#"{"address":{"country":"\u65e5\u672c"}}"#, 1), None);
    }

    #[test]
    fn tiles_decode_and_classify() {
        let e = decode_tile(Layer::Elevation, ELEV).unwrap();
        let (lo, hi) = (e.metres.iter().min().unwrap(), e.metres.iter().max().unwrap());
        assert_eq!((*lo, *hi), (-104, 2235));
        let m = decode_tile(Layer::Map, MAP).unwrap();
        assert_eq!(m.cover.len(), 65536);
        assert_eq!(m.rgba.len(), 65536 * 4);
        // The fixture has both water and plain land in it.
        let count = |c: u8| m.cover.iter().filter(|&&x| x == c).count();
        assert!(count(class::WATER) > 500, "water pixels: {}", count(class::WATER));
        assert!(count(class::LAND) + count(class::FOREST) + count(class::GRASS) > 500);
        assert!(decode_tile(Layer::Map, &MAP[..200]).is_none());
        assert_eq!(classify(170, 211, 223), class::WATER);
        assert_eq!(classify(172, 208, 160), class::FOREST);
        assert_eq!(classify(10, 10, 10), class::LAND);
    }

    #[test]
    fn terrain_follows_the_elevation_tile() {
        // A world whose origin is the top-left corner of the fixture tile
        // (zoom 8, tile 40/97), at one block per pixel.
        let p = Proj { zoom: 8, px0: 40.0 * 256.0, py0: 97.0 * 256.0, k: 1.0, lat0: 38.0, vscale: 20.0, scale: 400.0, features: true };
        insert_tile((Layer::Elevation, 8, 40, 97), decode_tile(Layer::Elevation, ELEV).unwrap());
        insert_tile((Layer::Map, 8, 40, 97), decode_tile(Layer::Map, MAP).unwrap());
        let e = decode_tile(Layer::Elevation, ELEV).unwrap();
        let mut seen_sea = false;
        let mut seen_land = false;
        for (x, z) in [(10, 10), (100, 40), (200, 200), (128, 128), (30, 220), (250, 5)] {
            let c = column(&p, x, z);
            let m = e.metres[(z * 256 + x) as usize] as f32;
            // Sampled at the pixel centre, so within a pixel's worth of the raw value.
            let want = metres_to_height(m, 20.0);
            assert!((c.height - want).abs() <= 4, "({x},{z}): {} vs {}", c.height, want);
            if m <= 0.0 {
                assert!(c.height < SEA);
                seen_sea = true;
            } else {
                assert!(c.height > SEA);
                seen_land = true;
            }
        }
        assert!(seen_land);
        let _ = seen_sea;
        assert_eq!(metres_to_height(-50.0, 25.0), SEA - 3);
        assert_eq!(metres_to_height(0.0, 25.0), SEA - 1);
        assert_eq!(metres_to_height(100.0, 25.0), SEA + 5);
        assert_eq!(metres_to_height(99_999.0, 25.0), CH - 14);
        assert_eq!(tile_url((Layer::Map, 3, 1, 2)), "https://tile.openstreetmap.org/3/1/2.png");
    }
}
