//! The look of a dimension's sky, as data.
//!
//! A `dimension` line may carry properties after its shape:
//!
//! ```text
//! dimension ember normal sky_colour 90,20,10 fog_colour #6a2a14 fog_distance 0.5 stars no clouds no sun no moon no ambient_light 8 time 0.5
//! ```
//!
//! | key | value | meaning |
//! | --- | --- | --- |
//! | `sky_colour` (`sky_color`) | `r,g,b` (0 to 255) or `#rrggbb`, or `none` | the colour overhead, fixed (`none`: the day and night cycle's) |
//! | `fog_colour` (`fog_color`) | the same | the colour of the horizon and the fog |
//! | `fog_distance` | a number, 0.1 to 4 | how far the fog is, times the usual (1 is the usual) |
//! | `stars`, `clouds`, `sun`, `moon` | `yes` or `no` | whether they are drawn |
//! | `ambient_light` | 0 to 15 | the least sky light: at 8 the sky never gets darker than half bright |
//! | `time` | 0 to 1, or `none` | the time of day the sky always shows (0 sunrise, 0.25 noon) |
//!
//! A script changes the same things while playing with `set_dimension_sky`
//! (see docs/SCRIPTING.md). Nothing here touches the game's own clock: mobs,
//! sleeping and saving go on with the real time of day; this is only what is
//! seen (and how bright the sky light looks).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// What a dimension's sky looks like. [`DimSky::DEFAULT`] is the ordinary one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DimSky {
    /// Fixed colour overhead; `None` follows the time of day.
    pub sky: Option<[u8; 3]>,
    /// Fixed colour of the horizon and the fog.
    pub fog: Option<[u8; 3]>,
    /// Times the usual fog distance.
    pub fog_scale: f32,
    pub stars: bool,
    pub clouds: bool,
    pub sun: bool,
    pub moon: bool,
    /// The least sky light, 0 to 15 (0: none, the sky goes as dark as it goes).
    pub ambient: u8,
    /// The time of day the sky always shows, 0 to 1.
    pub fixed_time: Option<f32>,
}

/// The sky of the home world and of dimensions that say nothing.
pub const DEFAULT: DimSky = DimSky { sky: None, fog: None, fog_scale: 1.0, stars: true, clouds: true, sun: true, moon: true, ambient: 0, fixed_time: None };

/// Smallest and largest fog scale.
pub const FOG_MIN: f32 = 0.1;
pub const FOG_MAX: f32 = 4.0;

/// Every key, for messages and for the docs' test.
pub const KEYS: &[&str] = &["sky_colour", "fog_colour", "fog_distance", "stars", "clouds", "sun", "moon", "ambient_light", "time"];

impl Default for DimSky {
    fn default() -> DimSky {
        DEFAULT
    }
}

/// `yes`, `no`, `true`, `false`, `on`, `off`, `1` and `0`.
pub fn parse_bool(s: &str) -> Option<bool> {
    match s.to_ascii_lowercase().as_str() {
        "yes" | "true" | "on" | "1" => Some(true),
        "no" | "false" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// `r,g,b` (each 0 to 255) or `#rrggbb` (or `#rgb`).
pub fn parse_colour(s: &str) -> Option<[u8; 3]> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let digit = |c: char| c.to_digit(16);
        let d: Vec<u32> = hex.chars().map(digit).collect::<Option<Vec<u32>>>()?;
        return match d.len() {
            6 => Some([(d[0] * 16 + d[1]) as u8, (d[2] * 16 + d[3]) as u8, (d[4] * 16 + d[5]) as u8]),
            3 => Some([(d[0] * 17) as u8, (d[1] * 17) as u8, (d[2] * 17) as u8]),
            _ => None,
        };
    }
    let mut out = [0u8; 3];
    let mut n = 0;
    for part in s.split(',') {
        if n >= 3 {
            return None;
        }
        let v = part.trim().parse::<i64>().ok()?;
        if !(0..=255).contains(&v) {
            return None;
        }
        out[n] = v as u8;
        n += 1;
    }
    if n == 3 {
        Some(out)
    } else {
        None
    }
}

/// A finite number from text.
fn parse_number(s: &str) -> Option<f32> {
    let v = s.trim().parse::<f32>().ok()?;
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

fn is_none_word(s: &str) -> bool {
    matches!(s.to_ascii_lowercase().as_str(), "none" | "off" | "default" | "auto" | "cycle" | "")
}

impl DimSky {
    pub const DEFAULT: DimSky = DEFAULT;

    /// Set one property from text. Unknown keys and values that make no
    /// sense are errors (and leave the sky as it was).
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let key = key.to_ascii_lowercase();
        match key.as_str() {
            "sky_colour" | "sky_color" | "sky" => {
                self.sky = if is_none_word(value) { None } else { Some(parse_colour(value).ok_or_else(|| format!("`{}` is not a colour (use r,g,b or #rrggbb)", value))?) };
            }
            "fog_colour" | "fog_color" | "fog" => {
                self.fog = if is_none_word(value) { None } else { Some(parse_colour(value).ok_or_else(|| format!("`{}` is not a colour (use r,g,b or #rrggbb)", value))?) };
            }
            "fog_distance" | "fog_scale" => {
                let v = parse_number(value).ok_or_else(|| format!("`{}` is not a number", value))?;
                self.fog_scale = v.clamp(FOG_MIN, FOG_MAX);
            }
            "stars" | "clouds" | "sun" | "moon" => {
                let b = parse_bool(value).ok_or_else(|| format!("`{}` should be yes or no", value))?;
                match key.as_str() {
                    "stars" => self.stars = b,
                    "clouds" => self.clouds = b,
                    "sun" => self.sun = b,
                    _ => self.moon = b,
                }
            }
            "ambient_light" | "ambient" => {
                let v = parse_number(value).ok_or_else(|| format!("`{}` is not a number", value))?;
                self.ambient = v.clamp(0.0, 15.0) as u8;
            }
            "time" | "fixed_time" | "time_of_day" => {
                self.fixed_time = if is_none_word(value) {
                    None
                } else {
                    let v = parse_number(value).ok_or_else(|| format!("`{}` is not a number from 0 to 1", value))?;
                    if !(0.0..=1.0).contains(&v) {
                        return Err(format!("the time of day `{}` is not from 0 to 1", value));
                    }
                    // 1 is the same moment as 0.
                    Some(if v >= 1.0 { 0.0 } else { v })
                };
            }
            _ => return Err(format!("unknown sky property `{}` (known: {})", key, KEYS.join(", "))),
        }
        Ok(())
    }

    /// Read the words after the shape of a `dimension` line: `key value` pairs.
    /// What is wrong is listed; the pairs that are right still count.
    pub fn parse_props(words: &[String]) -> (DimSky, Vec<String>) {
        let mut sky = DEFAULT;
        let mut problems = Vec::new();
        let mut i = 0;
        while i < words.len() {
            match words.get(i + 1) {
                Some(v) => {
                    if let Err(e) = sky.set(&words[i], v) {
                        problems.push(e);
                    }
                    i += 2;
                }
                None => {
                    problems.push(format!("`{}` has no value", words[i]));
                    i += 1;
                }
            }
        }
        (sky, problems)
    }

    /// Whether this is the ordinary sky (nothing set).
    pub fn is_default(&self) -> bool {
        *self == DEFAULT
    }

    /// The time of day to show, given the game's.
    pub fn time_of(&self, game_time: f32) -> f32 {
        self.fixed_time.unwrap_or(game_time)
    }

    /// The daylight (0 to 1) to light with, given what the clock gives: never
    /// below the dimension's ambient light.
    pub fn light(&self, day: f32) -> f32 {
        let floor = self.ambient as f32 / 15.0;
        if day < floor {
            floor
        } else {
            day
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn nothing_said_is_the_ordinary_sky() {
        let (sky, problems) = DimSky::parse_props(&[]);
        assert!(problems.is_empty());
        assert!(sky.is_default());
        assert_eq!(sky, DimSky::default());
        assert!(sky.stars && sky.clouds && sky.sun && sky.moon);
        assert_eq!(sky.fog_scale, 1.0);
        assert_eq!(sky.time_of(0.3), 0.3);
        assert_eq!(sky.light(0.2), 0.2);
    }

    #[test]
    fn every_property_is_read() {
        let (sky, problems) = DimSky::parse_props(&words("sky_colour 90,20,10 fog_colour #6a2a14 fog_distance 0.5 stars no clouds no sun off moon false ambient_light 8 time 0.5"));
        assert!(problems.is_empty(), "{:?}", problems);
        assert_eq!(sky.sky, Some([90, 20, 10]));
        assert_eq!(sky.fog, Some([0x6a, 0x2a, 0x14]));
        assert_eq!(sky.fog_scale, 0.5);
        assert!(!sky.stars && !sky.clouds && !sky.sun && !sky.moon);
        assert_eq!(sky.ambient, 8);
        assert_eq!(sky.fixed_time, Some(0.5));
        assert_eq!(sky.time_of(0.9), 0.5);
        assert!(!sky.is_default());
    }

    #[test]
    fn ambient_light_is_a_floor_for_the_day() {
        let mut sky = DimSky::default();
        sky.set("ambient_light", "15").unwrap();
        assert_eq!(sky.light(0.0), 1.0);
        sky.set("ambient_light", "6").unwrap();
        assert!((sky.light(0.0) - 0.4).abs() < 1e-6);
        assert_eq!(sky.light(0.9), 0.9);
        // Out of range is held to the range.
        sky.set("ambient_light", "99").unwrap();
        assert_eq!(sky.ambient, 15);
        sky.set("ambient_light", "-3").unwrap();
        assert_eq!(sky.ambient, 0);
    }

    #[test]
    fn colours_come_in_two_forms() {
        assert_eq!(parse_colour("255,0,128"), Some([255, 0, 128]));
        assert_eq!(parse_colour(" 1, 2 ,3 "), Some([1, 2, 3]));
        assert_eq!(parse_colour("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_colour("#f80"), Some([255, 136, 0]));
        assert_eq!(parse_colour("256,0,0"), None);
        assert_eq!(parse_colour("1,2"), None);
        assert_eq!(parse_colour("1,2,3,4"), None);
        assert_eq!(parse_colour("#12345"), None);
        assert_eq!(parse_colour("#gg0000"), None);
        assert_eq!(parse_colour("red"), None);
    }

    #[test]
    fn fog_distance_is_held_in_range() {
        let mut sky = DimSky::default();
        sky.set("fog_distance", "100").unwrap();
        assert_eq!(sky.fog_scale, FOG_MAX);
        sky.set("fog_distance", "0").unwrap();
        assert_eq!(sky.fog_scale, FOG_MIN);
        assert!(sky.set("fog_distance", "far").is_err());
        assert!(sky.set("fog_distance", "NaN").is_err());
    }

    #[test]
    fn mistakes_are_reported_and_the_rest_still_counts() {
        let (sky, problems) = DimSky::parse_props(&words("stars maybe bogus 1 clouds no time 2 sun"));
        assert_eq!(problems.len(), 4, "{:?}", problems);
        assert!(!sky.clouds);
        assert!(sky.stars, "a bad value leaves the old one");
        assert_eq!(sky.fixed_time, None);
    }

    #[test]
    fn colours_and_time_can_be_given_back() {
        let mut sky = DimSky::default();
        sky.set("sky_colour", "1,2,3").unwrap();
        sky.set("time", "0.25").unwrap();
        sky.set("sky_colour", "none").unwrap();
        sky.set("time", "cycle").unwrap();
        assert!(sky.is_default());
        // The end of the day is the start.
        sky.set("time", "1").unwrap();
        assert_eq!(sky.fixed_time, Some(0.0));
    }

    #[test]
    fn a_failed_set_changes_nothing() {
        let mut sky = DimSky::default();
        assert!(sky.set("fog_colour", "blue").is_err());
        assert!(sky.set("nothing", "1").is_err());
        assert!(sky.is_default());
    }

    #[test]
    fn the_docs_name_every_key() {
        let docs = include_str!("../../docs/DATA.md");
        for k in KEYS {
            assert!(docs.contains(k), "docs/DATA.md does not mention {}", k);
        }
    }
}
