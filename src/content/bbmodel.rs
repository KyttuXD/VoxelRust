//! Loader for Blockbench `.bbmodel` files.
//!
//! Reads the cubes, the bone tree from the outliner and the first embedded
//! texture. Units are left as they are in the file (model units and texture
//! pixels); nothing is transformed apart from applying each cube's `inflate`.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::json::{self, Json};
use crate::png;

#[derive(Debug, Clone, PartialEq)]
pub struct Cube {
    pub name: String,
    /// Corners in model units, `inflate` already applied.
    pub from: [f32; 3],
    pub to: [f32; 3],
    /// The cube's own pivot, and its rotation about it in degrees.
    pub origin: [f32; 3],
    pub rotation: [f32; 3],
    /// Per face: uv rectangle in texture pixels `[x1, y1, x2, y2]` and
    /// rotation in quarter turns; `None` = face not drawn.
    /// Order: 0 east(+X), 1 west(-X), 2 up(+Y), 3 down(-Y), 4 south(+Z), 5 north(-Z).
    pub faces: [Option<([f32; 4], u8)>; 6],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    /// Pivot in model units, and rotation about it in degrees.
    pub origin: [f32; 3],
    pub rotation: [f32; 3],
    /// Indices into [`Model::cubes`].
    pub cubes: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// Parents always come before children.
    pub bones: Vec<Bone>,
    pub cubes: Vec<Cube>,
    /// The uv resolution.
    pub tex_w: f32,
    pub tex_h: f32,
    /// First embedded texture, decoded.
    pub texture: Option<png::Image>,
    /// Bounding box of all cubes (unrotated); zero when there are none.
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Model {
    /// Index of the first bone with this name.
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }
}

/// Name of the bone that collects cubes no outliner group refers to. It is
/// only added, as the last bone, when there are such cubes.
pub const ROOT_BONE: &str = "root";

const FACE_NAMES: [&str; 6] = ["east", "west", "up", "down", "south", "north"];

/// An array of exactly `N` finite numbers.
fn floats<const N: usize>(v: &Json) -> Option<[f32; N]> {
    let arr = v.as_arr()?;
    if arr.len() != N {
        return None;
    }
    let mut out = [0.0f32; N];
    for (o, item) in out.iter_mut().zip(arr) {
        let f = item.as_f64()? as f32;
        if !f.is_finite() {
            return None;
        }
        *o = f;
    }
    Some(out)
}

/// A vector member that may be absent (or null), in which case it is zero.
fn vec3_or_zero(obj: &Json, key: &str, what: &str) -> Result<[f32; 3], String> {
    match obj.get(key) {
        None | Some(Json::Null) => Ok([0.0; 3]),
        Some(v) => floats::<3>(v).ok_or_else(|| format!("bbmodel: {} has a bad \"{}\"", what, key)),
    }
}

fn name_of(obj: &Json) -> String {
    obj.get("name").and_then(Json::as_str).unwrap_or("").into()
}

fn parse_face(face: Option<&Json>) -> Option<([f32; 4], u8)> {
    let face = face?;
    if let Some(Json::Null) = face.get("texture") {
        return None;
    }
    let uv = floats::<4>(face.get("uv")?)?;
    let turns = match face.get("rotation").and_then(Json::as_f64) {
        Some(deg) => ((deg / 90.0) as i32).rem_euclid(4) as u8,
        None => 0,
    };
    Some((uv, turns))
}

fn parse_cube(e: &Json, index: usize) -> Result<Cube, String> {
    let name = name_of(e);
    let what = format!("element {} ({:?})", index, name);
    let corner = |key: &str| -> Result<[f32; 3], String> {
        e.get(key)
            .and_then(floats::<3>)
            .ok_or_else(|| format!("bbmodel: {} has a missing or bad \"{}\"", what, key))
    };
    let (mut from, mut to) = (corner("from")?, corner("to")?);
    let inflate = match e.get("inflate") {
        None | Some(Json::Null) => 0.0,
        Some(v) => match v.as_f64() {
            Some(f) if f.is_finite() => f as f32,
            _ => return Err(format!("bbmodel: {} has a bad \"inflate\"", what)),
        },
    };
    for a in 0..3 {
        from[a] -= inflate;
        to[a] += inflate;
    }
    let mut faces = [None; 6];
    if let Some(list) = e.get("faces") {
        for (slot, face_name) in faces.iter_mut().zip(FACE_NAMES) {
            *slot = parse_face(list.get(face_name));
        }
    }
    Ok(Cube {
        origin: vec3_or_zero(e, "origin", &what)?,
        rotation: vec3_or_zero(e, "rotation", &what)?,
        name,
        from,
        to,
        faces,
    })
}

struct Outline<'a> {
    by_uuid: BTreeMap<&'a str, usize>,
    /// Which cubes already belong to a bone.
    placed: Vec<bool>,
    bones: Vec<Bone>,
}

impl<'a> Outline<'a> {
    /// Walks one outliner node. Recursion is bounded by the JSON depth cap.
    fn walk(&mut self, node: &Json, parent: Option<usize>) -> Result<(), String> {
        if let Some(uuid) = node.as_str() {
            // Unknown ids (skipped non-cube elements) and top-level cubes
            // are left for the root bone.
            if let (Some(bone), Some(&cube)) = (parent, self.by_uuid.get(uuid)) {
                if !self.placed[cube] {
                    self.placed[cube] = true;
                    self.bones[bone].cubes.push(cube);
                }
            }
            return Ok(());
        }
        if node.as_obj().is_none() {
            return Err("bbmodel: outliner node is neither a string nor an object".into());
        }
        let name = name_of(node);
        let what = format!("group {:?}", name);
        let index = self.bones.len();
        self.bones.push(Bone {
            origin: vec3_or_zero(node, "origin", &what)?,
            rotation: vec3_or_zero(node, "rotation", &what)?,
            name,
            parent,
            cubes: Vec::new(),
        });
        if let Some(children) = node.get("children").and_then(Json::as_arr) {
            for child in children {
                self.walk(child, Some(index))?;
            }
        }
        Ok(())
    }
}

/// The first texture that carries its image as a base64 PNG data URI.
fn embedded_texture(root: &Json) -> Result<Option<png::Image>, String> {
    let textures = match root.get("textures").and_then(Json::as_arr) {
        Some(t) => t,
        None => return Ok(None),
    };
    for tex in textures {
        let source = match tex.get("source").and_then(Json::as_str) {
            Some(s) if s.starts_with("data:") => s,
            _ => continue,
        };
        let image = source
            .find("base64,")
            .and_then(|at| source.get(at + 7..))
            .and_then(png::base64_decode)
            .and_then(|bytes| png::decode(&bytes));
        return match image {
            Some(image) => Ok(Some(image)),
            None => Err("bbmodel: embedded texture is not a readable PNG".into()),
        };
    }
    Ok(None)
}

/// Parses the text of a `.bbmodel` file.
pub fn load(text: &str) -> Result<Model, String> {
    let root = json::parse(text)?;
    if root.as_obj().is_none() {
        return Err("bbmodel: top level is not an object".into());
    }
    let elements = root
        .get("elements")
        .and_then(Json::as_arr)
        .ok_or_else(|| String::from("bbmodel: missing \"elements\" array"))?;

    let mut cubes = Vec::new();
    let mut by_uuid = BTreeMap::new();
    for (i, e) in elements.iter().enumerate() {
        if e.as_obj().is_none() {
            return Err(format!("bbmodel: element {} is not an object", i));
        }
        match e.get("type") {
            None => {}
            Some(t) if t.as_str() == Some("cube") => {}
            Some(_) => continue,
        }
        if let Some(uuid) = e.get("uuid").and_then(Json::as_str) {
            by_uuid.entry(uuid).or_insert(cubes.len());
        }
        cubes.push(parse_cube(e, i)?);
    }

    let mut outline = Outline { by_uuid, placed: alloc::vec![false; cubes.len()], bones: Vec::new() };
    match root.get("outliner") {
        None | Some(Json::Null) => {}
        Some(Json::Arr(nodes)) => {
            for node in nodes {
                outline.walk(node, None)?;
            }
        }
        Some(_) => return Err("bbmodel: \"outliner\" is not an array".into()),
    }
    let Outline { placed, mut bones, .. } = outline;
    let loose: Vec<usize> = (0..cubes.len()).filter(|&i| !placed[i]).collect();
    if !loose.is_empty() {
        bones.push(Bone {
            name: ROOT_BONE.into(),
            parent: None,
            origin: [0.0; 3],
            rotation: [0.0; 3],
            cubes: loose,
        });
    }

    let texture = embedded_texture(&root)?;
    let side = |key: &str, fallback: Option<usize>| -> f32 {
        let listed = root.get("resolution").and_then(|r| r.get(key)).and_then(Json::as_f64);
        match (listed, fallback) {
            (Some(v), _) if v >= 1.0 && v <= 65536.0 => v as f32,
            (_, Some(px)) => px as f32,
            _ => 16.0,
        }
    };
    let tex_w = side("width", texture.as_ref().map(|t| t.w));
    let tex_h = side("height", texture.as_ref().map(|t| t.h));

    let (mut min, mut max) = ([0.0f32; 3], [0.0f32; 3]);
    for (i, c) in cubes.iter().enumerate() {
        for a in 0..3 {
            let (lo, hi) = (c.from[a].min(c.to[a]), c.from[a].max(c.to[a]));
            min[a] = if i == 0 { lo } else { min[a].min(lo) };
            max[a] = if i == 0 { hi } else { max[a].max(hi) };
        }
    }

    Ok(Model { bones, cubes, tex_w, tex_h, texture, min, max })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER: &str = include_str!("../../data/models/player.bbmodel");

    #[test]
    fn player_model() {
        let m = load(PLAYER).unwrap();
        assert_eq!(m.cubes.len(), 42);
        assert_eq!((m.tex_w, m.tex_h), (64.0, 64.0));
        let tex = m.texture.as_ref().unwrap();
        assert_eq!((tex.w, tex.h, tex.rgba.len()), (64, 64, 64 * 64 * 4));
        // The embedded texture is the same picture as the atlas fixture.
        let atlas = png::decode(include_bytes!("../../tests/fixtures/player_atlas.png")).unwrap();
        assert_eq!(tex, &atlas);

        let tree = [
            ("Pelvis", None),
            ("Belly", Some("Pelvis")),
            ("Chest", Some("Belly")),
            ("Head", Some("Chest")),
            ("L-Arm", Some("Chest")),
            ("L-Forearm", Some("L-Arm")),
            ("L-Hand", Some("L-Forearm")),
            ("R-Arm", Some("Chest")),
            ("R-Forearm", Some("R-Arm")),
            ("R-Hand", Some("R-Forearm")),
            ("L-Thigh", Some("Pelvis")),
            ("L-Calf", Some("L-Thigh")),
            ("L-Foot", Some("L-Calf")),
            ("R-Thigh", Some("Pelvis")),
            ("R-Calf", Some("R-Thigh")),
            ("R-Foot", Some("R-Calf")),
        ];
        assert_eq!(m.bones.len(), tree.len());
        for (i, (name, parent)) in tree.iter().enumerate() {
            assert_eq!(m.bones[i].name, *name);
            assert_eq!(m.bone(name), Some(i));
            assert_eq!(m.bones[i].parent, parent.and_then(|p| m.bone(p)), "{}", name);
            if let Some(p) = m.bones[i].parent {
                assert!(p < i, "parent of {} must come first", name);
            }
        }
        assert!(m.bone("Tail").is_none());
        assert_eq!(m.bones[m.bone("Head").unwrap()].origin, [0.0, 85.44, 0.0]);
        assert_eq!(m.bones[m.bone("L-Arm").unwrap()].origin, [-21.36, 78.32, 0.0]);

        // Every cube sits in exactly one bone.
        let mut seen = vec![0; m.cubes.len()];
        for b in &m.bones {
            for &c in &b.cubes {
                seen[c] += 1;
            }
        }
        assert!(seen.iter().all(|&n| n == 1));
        assert!(m.bones[0].cubes.is_empty());

        let head_bone = &m.bones[m.bone("Head").unwrap()];
        assert_eq!(head_bone.cubes.len(), 2);
        let head = &m.cubes[head_bone.cubes[0]];
        assert_eq!(head.name, "head");
        assert_eq!(head.from, [-14.24, 85.44, -14.24]);
        assert_eq!(head.to, [14.24, 113.92, 14.24]);
        assert_eq!(head.origin, [0.0, 99.68, 0.0]);
        assert_eq!(head.rotation, [0.0; 3]);
        assert_eq!(head.faces[5], Some(([8.0, 8.0, 16.0, 16.0], 0))); // north
        assert_eq!(head.faces[4], Some(([24.0, 8.0, 32.0, 16.0], 0))); // south
        assert_eq!(head.faces[0], Some(([0.0, 8.0, 8.0, 16.0], 0))); // east
        assert_eq!(head.faces[1], Some(([16.0, 8.0, 24.0, 16.0], 0))); // west
        assert_eq!(head.faces[2], Some(([8.0, 0.0, 16.0, 8.0], 0))); // up
        assert_eq!(head.faces[3], Some(([16.0, 0.0, 24.0, 8.0], 0))); // down

        // The bounding box really bounds, and is touched on every side.
        for a in 0..3 {
            assert!(m.cubes.iter().all(|c| c.from[a] >= m.min[a] && c.to[a] <= m.max[a]));
            assert!(m.cubes.iter().any(|c| c.from[a] == m.min[a]));
            assert!(m.cubes.iter().any(|c| c.to[a] == m.max[a]));
        }
        assert_eq!(m.min, [-29.37, 0.0, -16.02]);
        assert_eq!(m.max, [29.37, 115.7, 16.02]); // the hat tops the head
    }

    const SMALL: &str = r#"{
        "meta": {"format_version": "4.10", "model_format": "free", "box_uv": false},
        "resolution": {"width": 32, "height": 16},
        "elements": [
            {"name": "arm", "type": "cube", "uuid": "a", "from": [0, 1, 2], "to": [4, 5, 6],
             "origin": [1, 2, 3], "rotation": [0, 45, -22.5], "inflate": 0.5,
             "faces": {
                "north": {"uv": [0, 0, 4, 4], "texture": 0, "rotation": 90},
                "south": {"uv": [4, 0, 8, 4], "texture": null},
                "east":  {"texture": 0},
                "west":  {"uv": [8, 0, 4, 4], "rotation": 270},
                "up":    {"uv": [1.5, 2.5, 3.5, 4.5], "texture": 0, "rotation": 180}
             }},
            {"name": "blob", "type": "mesh", "uuid": "m", "vertices": {}},
            {"name": "loose", "uuid": "l", "from": [-8, 0, -1], "to": [-6, 1, 1]},
            {"name": "top", "type": "cube", "uuid": "t", "from": [0, 9, 0], "to": [1, 10, 1], "faces": {}}
        ],
        "outliner": [
            "t",
            {"name": "body", "origin": [0, 1, 0], "children": [
                "m",
                {"name": "limb", "origin": [1, 2, 3], "rotation": [10, 20, 30], "children": ["a", "a", "nope"]}
            ]}
        ],
        "textures": [{"name": "skin.png", "source": "skin.png"}]
    }"#;

    #[test]
    fn hand_written_model() {
        let m = load(SMALL).unwrap();
        assert_eq!(m.cubes.len(), 3); // the mesh is skipped
        assert_eq!((m.tex_w, m.tex_h), (32.0, 16.0));
        assert!(m.texture.is_none()); // a file path, not embedded

        let arm = &m.cubes[0];
        assert_eq!(arm.name, "arm");
        assert_eq!(arm.from, [-0.5, 0.5, 1.5]);
        assert_eq!(arm.to, [4.5, 5.5, 6.5]);
        assert_eq!(arm.origin, [1.0, 2.0, 3.0]);
        assert_eq!(arm.rotation, [0.0, 45.0, -22.5]);
        assert_eq!(arm.faces[5], Some(([0.0, 0.0, 4.0, 4.0], 1))); // north, rotated 90
        assert_eq!(arm.faces[4], None); // south: texture null
        assert_eq!(arm.faces[0], None); // east: no uv
        assert_eq!(arm.faces[1], Some(([8.0, 0.0, 4.0, 4.0], 3))); // west: no texture key, flipped uv
        assert_eq!(arm.faces[2], Some(([1.5, 2.5, 3.5, 4.5], 2))); // up
        assert_eq!(arm.faces[3], None); // down: absent

        assert_eq!(m.cubes[1].name, "loose");
        assert_eq!(m.cubes[1].faces, [None; 6]);
        assert_eq!(m.cubes[1].rotation, [0.0; 3]);

        let names: Vec<&str> = m.bones.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["body", "limb", ROOT_BONE]);
        assert_eq!(m.bones[0].parent, None);
        assert_eq!(m.bones[0].origin, [0.0, 1.0, 0.0]);
        assert_eq!(m.bones[0].rotation, [0.0; 3]);
        assert!(m.bones[0].cubes.is_empty());
        assert_eq!(m.bones[1].parent, Some(0));
        assert_eq!(m.bones[1].rotation, [10.0, 20.0, 30.0]);
        assert_eq!(m.bones[1].cubes, [0]); // listed twice, placed once
        assert_eq!(m.bones[2].parent, None);
        assert_eq!(m.bones[2].cubes, [1, 2]); // not in the outliner, and top-level

        assert_eq!(m.min, [-8.0, 0.0, -1.0]);
        assert_eq!(m.max, [4.5, 10.0, 6.5]);
    }

    #[test]
    fn minimal_models() {
        let m = load(r#"{"elements": []}"#).unwrap();
        assert!(m.bones.is_empty() && m.cubes.is_empty() && m.texture.is_none());
        assert_eq!((m.tex_w, m.tex_h, m.min, m.max), (16.0, 16.0, [0.0; 3], [0.0; 3]));

        // No outliner: everything hangs off the root bone. The embedded
        // texture supplies the resolution when the file gives none.
        let png = png::encode(2, 3, &[9; 24]);
        let b64 = {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for c in png.chunks(3) {
                let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
                for k in 0..4 {
                    s.push(if k <= c.len() { A[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
                }
            }
            s
        };
        let text = format!(
            r#"{{"elements":[{{"from":[0,0,0],"to":[1,2,3]}}],
                "textures":[{{"source":"a.png"}},{{"source":"data:image/png;base64,{}"}}]}}"#,
            b64
        );
        let m = load(&text).unwrap();
        assert_eq!(m.bones.len(), 1);
        assert_eq!(m.bone(ROOT_BONE), Some(0));
        assert_eq!(m.bones[0].cubes, [0]);
        assert_eq!((m.tex_w, m.tex_h), (2.0, 3.0));
        assert_eq!(m.texture, Some(png::Image { w: 2, h: 3, rgba: vec![9; 24] }));
        assert_eq!((m.min, m.max), ([0.0; 3], [1.0, 2.0, 3.0]));
    }

    #[test]
    fn malformed_inputs() {
        for bad in [
            "",
            "not json",
            "[]",
            "{}",
            r#"{"elements": 5}"#,
            r#"{"elements": [7]}"#,
            r#"{"elements": [{"type": "cube"}]}"#,
            r#"{"elements": [{"from": [0, 0, 0]}]}"#,
            r#"{"elements": [{"from": [0, 0], "to": [1, 1, 1]}]}"#,
            r#"{"elements": [{"from": [0, 0, "x"], "to": [1, 1, 1]}]}"#,
            r#"{"elements": [{"from": [0, 0, 1e99], "to": [1, 1, 1]}]}"#,
            r#"{"elements": [{"from": [0, 0, 0], "to": [1, 1, 1], "rotation": [0, 0]}]}"#,
            r#"{"elements": [{"from": [0, 0, 0], "to": [1, 1, 1], "origin": "x"}]}"#,
            r#"{"elements": [{"from": [0, 0, 0], "to": [1, 1, 1], "inflate": "x"}]}"#,
            r#"{"elements": [], "outliner": {}}"#,
            r#"{"elements": [], "outliner": [5]}"#,
            r#"{"elements": [], "outliner": [{"name": "a", "origin": [1]}]}"#,
            r#"{"elements": [], "outliner": [{"name": "a", "children": [null]}]}"#,
            r#"{"elements": [], "textures": [{"source": "data:image/png;base64,AAAA"}]}"#,
            r#"{"elements": [], "textures": [{"source": "data:image/png;base64,@@"}]}"#,
            r#"{"elements": [], "textures": [{"source": "data:image/png,raw"}]}"#,
        ] {
            assert!(load(bad).is_err(), "should reject {:?}", bad);
        }
        // A truncated real file is an error, never a panic.
        for cut in [1, 100, 5000, PLAYER.len() / 2, PLAYER.len() - 1] {
            assert!(load(&PLAYER[..cut]).is_err());
        }
    }
}
