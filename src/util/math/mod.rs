//! Small math library.
//!
//! `core` has no `sin`, `sqrt` or `floor` (those live in `std`, which we do
//! not use), so the few we need are written here. They are plain arithmetic,
//! which also makes world generation bit-identical on every machine.

pub mod noise;

use core::ops::{Add, Mul, Sub};

pub const PI: f32 = core::f32::consts::PI;
pub const TAU: f32 = 2.0 * PI;

#[inline]
pub fn absf(x: f32) -> f32 {
    if x < 0.0 {
        -x
    } else {
        x
    }
}

#[inline]
pub fn floori(x: f32) -> i32 {
    let i = x as i32;
    if (i as f32) > x {
        i - 1
    } else {
        i
    }
}

#[inline]
pub fn floord(x: f64) -> i32 {
    let i = x as i32;
    if (i as f64) > x {
        i - 1
    } else {
        i
    }
}

#[inline]
pub fn floorf(x: f32) -> f32 {
    floori(x) as f32
}

#[inline]
pub fn clampf(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 0 below `e0`, 1 above `e1`, smooth in between.
#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clampf((x - e0) / (e1 - e0), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn sqrtf(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    // Bit-trick first guess, then Newton's method.
    let mut y = f32::from_bits((x.to_bits() >> 1) + 0x1FBD_1DF5);
    y = 0.5 * (y + x / y);
    y = 0.5 * (y + x / y);
    y = 0.5 * (y + x / y);
    y
}

pub fn sinf(x: f32) -> f32 {
    // Reduce to [-pi, pi], fold to [-pi/2, pi/2], then a degree-9 polynomial.
    let k = floorf(x * (1.0 / TAU) + 0.5);
    let mut r = x - k * TAU;
    if r > PI * 0.5 {
        r = PI - r;
    } else if r < -PI * 0.5 {
        r = -PI - r;
    }
    let r2 = r * r;
    r * (1.0 + r2 * (-1.0 / 6.0 + r2 * (1.0 / 120.0 + r2 * (-1.0 / 5040.0 + r2 * (1.0 / 362_880.0)))))
}

#[inline]
pub fn cosf(x: f32) -> f32 {
    sinf(x + PI * 0.5)
}

/// The angle of the vector (x, y), in -pi..=pi. Accurate to about 0.0002
/// radians, which is plenty for steering.
pub fn atan2f(y: f32, x: f32) -> f32 {
    let (ax, ay) = (absf(x), absf(y));
    if ax + ay == 0.0 {
        return 0.0;
    }
    // atan of a ratio in 0..=1, by a polynomial.
    let (lo, hi) = if ax < ay { (ax, ay) } else { (ay, ax) };
    let t = lo / hi;
    let t2 = t * t;
    let mut a = t * (0.999_866 + t2 * (-0.330_299_5 + t2 * (0.180_141 + t2 * (-0.085_133 + t2 * 0.020_835_1))));
    if ay > ax {
        a = PI * 0.5 - a;
    }
    if x < 0.0 {
        a = PI - a;
    }
    if y < 0.0 {
        -a
    } else {
        a
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn len(self) -> f32 {
        sqrtf(self.dot(self))
    }
    pub fn norm(self) -> Vec3 {
        let l = self.len();
        if l > 1e-6 {
            self * (1.0 / l)
        } else {
            self
        }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

/// Unit vector the camera looks along. Yaw 0 looks down -Z; positive yaw
/// turns left; positive pitch looks up.
pub fn forward(yaw: f32, pitch: f32) -> Vec3 {
    let cp = cosf(pitch);
    Vec3::new(-sinf(yaw) * cp, sinf(pitch), -cosf(yaw) * cp)
}

/// Column-major 4x4 matrix, as OpenGL wants it.
pub type Mat4 = [f32; 16];

pub const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + r] * b[c * 4 + k];
            }
            m[c * 4 + r] = s;
        }
    }
    m
}

pub fn perspective(fovy_deg: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let half = fovy_deg * PI / 360.0;
    let f = cosf(half) / sinf(half);
    let mut m = [0.0f32; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = (far + near) / (near - far);
    m[11] = -1.0;
    m[14] = 2.0 * far * near / (near - far);
    m
}

pub fn ortho(w: f32, h: f32) -> Mat4 {
    // Pixel coordinates, origin top-left.
    let mut m = IDENTITY;
    m[0] = 2.0 / w;
    m[5] = -2.0 / h;
    m[10] = -1.0;
    m[12] = -1.0;
    m[13] = 1.0;
    m
}

/// Rotation part of the view matrix (the camera sits at the origin).
pub fn view_rotation(yaw: f32, pitch: f32) -> Mat4 {
    let f = forward(yaw, pitch);
    let r = Vec3::new(cosf(yaw), 0.0, -sinf(yaw));
    let u = r.cross(f);
    [r.x, u.x, -f.x, 0.0, r.y, u.y, -f.y, 0.0, r.z, u.z, -f.z, 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// `m * translate(t) * scale(s)`.
pub fn translate_scale(m: &Mat4, t: Vec3, s: f32) -> Mat4 {
    let mut o = *m;
    for r in 0..3 {
        o[12 + r] = m[r] * t.x + m[4 + r] * t.y + m[8 + r] * t.z + m[12 + r];
    }
    for i in 0..12 {
        o[i] *= s;
    }
    o
}

/// Six clip planes, for culling boxes that are certainly off screen.
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    pub fn new(clip: &Mat4) -> Frustum {
        let row = |i: usize| [clip[i], clip[4 + i], clip[8 + i], clip[12 + i]];
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
        let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
        Frustum { planes: [add(r3, r0), sub(r3, r0), add(r3, r1), sub(r3, r1), add(r3, r2), sub(r3, r2)] }
    }

    pub fn box_visible(&self, min: Vec3, max: Vec3) -> bool {
        for p in &self.planes {
            let x = if p[0] >= 0.0 { max.x } else { min.x };
            let y = if p[1] >= 0.0 { max.y } else { min.y };
            let z = if p[2] >= 0.0 { max.z } else { min.z };
            if p[0] * x + p[1] * y + p[2] * z + p[3] < 0.0 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trig_matches_std() {
        let mut x = -20.0f32;
        while x < 20.0 {
            assert!((sinf(x) - x.sin()).abs() < 2e-5, "sin({x})");
            assert!((cosf(x) - x.cos()).abs() < 2e-5, "cos({x})");
            x += 0.013;
        }
    }

    #[test]
    fn atan2_matches_std() {
        for i in -20..=20 {
            for j in -20..=20 {
                let (y, x) = (i as f32 * 0.37, j as f32 * 0.41);
                assert!((atan2f(y, x) - y.atan2(x)).abs() < 3e-4 || (i == 0 && j == 0), "atan2({y}, {x})");
            }
        }
    }

    #[test]
    fn sqrt_matches_std() {
        for &x in &[1e-4f32, 0.5, 1.0, 2.0, 9.0, 1234.5, 1e6] {
            assert!((sqrtf(x) - x.sqrt()).abs() <= x.sqrt() * 1e-5, "sqrt({x})");
        }
        assert_eq!(sqrtf(0.0), 0.0);
    }

    #[test]
    fn floor_handles_negatives() {
        assert_eq!(floori(-0.5), -1);
        assert_eq!(floori(-1.0), -1);
        assert_eq!(floori(1.9), 1);
        assert_eq!(floord(-16.0001), -17);
    }

    #[test]
    fn frustum_culls_behind_camera() {
        let clip = mat_mul(&perspective(70.0, 1.5, 0.1, 100.0), &view_rotation(0.0, 0.0));
        let f = Frustum::new(&clip);
        // Looking down -Z: a box in front is visible, one behind is not.
        assert!(f.box_visible(Vec3::new(-1.0, -1.0, -10.0), Vec3::new(1.0, 1.0, -8.0)));
        assert!(!f.box_visible(Vec3::new(-1.0, -1.0, 8.0), Vec3::new(1.0, 1.0, 10.0)));
    }
}
