//! SK-VM's handset 3D package, called `m`.
//!
//! One local title, 크레이지버스, draws its world through three classes in a
//! package named `m`: a vector [`V3`], an affine transform [`A3`], and a
//! renderer [`XO_World`]. Without them the title's Canvas dies in its own class
//! initializer, before it paints anything - `NoClassDefFoundError: m/V3`.
//!
//! `V3` and `A3` are arithmetic: a title composes a camera, transforms a point,
//! and reads the result back as three integers it projects itself, and all of
//! that is answered exactly. `XO_World` is the renderer - the part the reference
//! emulator (wfeature, `internal/api/skvm/micro3d.go`) left as a stub for want
//! of a rasterizer and the two model formats. Here it is real: it parses the
//! uncompressed version-3 `.mbac` model and its `.bmp` skin and draws the mesh
//! with a small software rasterizer (see the `model` submodule), so the 3D
//! character appears. The `.mtra` motion is decoded too, so the posture a title
//! selects poses the model's bones rather than drawing it in its rest pose.
//!
//! The fixed point is the title's own: a coordinate is scaled so 4096 is 1.0,
//! and a full circle is 4096 of the angle unit the trigonometry takes.

mod a3;
mod model;
mod v3;
mod xo_world;

pub use {a3::A3, v3::V3, xo_world::XoWorld};

/// The fixed-point unit: 4096 is 1.0.
pub(crate) const MICRO3D_ONE: i64 = 4096;

/// A full circle in the angle unit the trigonometry takes.
pub(crate) const MICRO3D_TURN: f64 = 4096.0;

/// `sin`/`cos` on the title's angle unit, scaled back to the fixed point.
pub(crate) fn trig(angle: i32, of: fn(f64) -> f64) -> i32 {
    let radians = angle as f64 * 2.0 * core::f64::consts::PI / MICRO3D_TURN;
    libm::round(of(radians) * MICRO3D_ONE as f64) as i32
}
