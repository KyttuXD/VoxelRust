//! Post-processing shaders that mods bring (see docs/SHADERS.md).
//!
//! The 3D view is drawn as usual, copied to a texture, and then a chain of
//! one to three full-screen fragment shaders turns it into the picture the
//! player sees (the interface is drawn afterwards, untouched). With no
//! shader chosen none of this runs and the normal path costs nothing.
//!
//! * [`names`]: shader ids and the names of the uniforms scripts may set.
//! * [`plan`]: which pass draws where (pure logic).
//! * [`source`]: the fixed vertex shader and the header put before every
//!   fragment shader.
//! * [`post`]: compiling, and the passes themselves.
//! * [`hook`]: what the app hands the renderer each frame.
//! * [`menu`]: the SHADER row of SETTINGS > VIDEO.

pub mod hook;
pub mod menu;
pub mod names;
pub mod plan;
pub mod post;
pub mod source;
