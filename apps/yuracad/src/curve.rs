//! Distance function of the curve (a tube through `Shape::path` nodes), in the shape's own space.
//! The shader copy is `shader_curve.wgsl`; keep the two in step.

use crate::sdf::{Prepared, V3, len};

/// Distance from `q` (local, unscaled) to the shape. Negative inside.
pub fn distance(p: &Prepared, q: V3) -> f32 {
    len(q) - p.size[0]
}
