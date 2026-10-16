//! Distance functions of the CAD features (Revolve, Loft, Curve, Sweep), in the shape's own space.
//! The shader copy is `shader_cad.wgsl`; keep the two in step.

use crate::sdf::{Prepared, V3, len};

/// Distance from `q` (local, unscaled) to the shape. Negative inside.
pub fn distance(p: &Prepared, q: V3) -> f32 {
    len(q) - p.size[0]
}
