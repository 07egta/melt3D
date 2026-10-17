//! The view: orbit around a target, Z up. Perspective or orthographic.

use crate::sdf::{V3, add, cross, dot, mul, norm, sub};
use egui::{Pos2, Rect, pos2};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: V3,
    /// Turn about Z (radians). 0 looks along +Y (the front view).
    pub yaw: f32,
    /// Lift above the ground (radians).
    pub pitch: f32,
    pub dist: f32,
    pub ortho: bool,
}

/// tan(fov / 2) of the perspective view.
pub const TAN_HALF_FOV: f32 = 0.45;
const ORTHO_BACK: f32 = 400.0;
const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.002;
/// Mouse deltas are egui logical points; limiting one event prevents jumps on pointer re-entry.
const MAX_DRAG_STEP: f32 = 160.0;
const ORBIT_RAD_PER_POINT: f32 = 0.0065;
const MAX_SCROLL_STEP: f32 = 400.0;

impl Default for Camera {
    fn default() -> Self {
        Camera { target: [0.0, 0.0, 15.0], yaw: 0.6, pitch: 0.42, dist: 150.0, ortho: false }
    }
}

impl Camera {
    fn dir(&self) -> V3 {
        [self.yaw.sin() * self.pitch.cos(), -self.yaw.cos() * self.pitch.cos(), self.pitch.sin()]
    }
    pub fn eye(&self) -> V3 {
        let back = if self.ortho { self.dist + ORTHO_BACK } else { self.dist };
        add(self.target, mul(self.dir(), back))
    }
    /// (right, up, forward)
    pub fn basis(&self) -> (V3, V3, V3) {
        let fwd = mul(self.dir(), -1.0);
        let right = norm(cross(fwd, [0.0, 0.0, 1.0]));
        let up = cross(right, fwd);
        (right, up, fwd)
    }
    /// Half the view's height in mm at the target (orthographic).
    pub fn half_height(&self) -> f32 {
        self.dist * TAN_HALF_FOV
    }
    /// Millimetres per screen pixel at `p`.
    pub fn mm_per_px(&self, p: V3, height_px: f32) -> f32 {
        let h = height_px.max(1.0);
        if self.ortho {
            2.0 * self.half_height() / h
        } else {
            let (_, _, fwd) = self.basis();
            let z = dot(sub(p, self.eye()), fwd).max(1.0);
            2.0 * z * TAN_HALF_FOV / h
        }
    }
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.yaw -= dx.clamp(-MAX_DRAG_STEP, MAX_DRAG_STEP) * ORBIT_RAD_PER_POINT;
        self.pitch = (self.pitch + dy.clamp(-MAX_DRAG_STEP, MAX_DRAG_STEP) * ORBIT_RAD_PER_POINT).clamp(-MAX_PITCH, MAX_PITCH);
    }
    pub fn pan(&mut self, dx: f32, dy: f32, height_px: f32) {
        if !dx.is_finite() || !dy.is_finite() || !height_px.is_finite() {
            return;
        }
        let (right, up, _) = self.basis();
        let k = self.mm_per_px(self.target, height_px);
        self.target =
            add(self.target, add(mul(right, -dx.clamp(-MAX_DRAG_STEP, MAX_DRAG_STEP) * k), mul(up, dy.clamp(-MAX_DRAG_STEP, MAX_DRAG_STEP) * k)));
    }
    pub fn zoom(&mut self, scroll: f32) {
        if !scroll.is_finite() {
            return;
        }
        self.dist = (self.dist * (-scroll.clamp(-MAX_SCROLL_STEP, MAX_SCROLL_STEP) * 0.0015).exp()).clamp(4.0, 4000.0);
    }
    /// The point on the plane through the target (facing the viewer) that lies under screen point `p`.
    fn focus_point(&self, rect: Rect, p: Pos2) -> Option<V3> {
        let (o, d) = self.ray_at(rect, p);
        let (_, _, fwd) = self.basis();
        let den = dot(d, fwd);
        if den < 1e-4 {
            return None;
        }
        let t = dot(sub(self.target, o), fwd) / den;
        t.is_finite().then(|| add(o, mul(d, t)))
    }
    /// Zoom, keeping the thing under the mouse where it is.
    pub fn zoom_at(&mut self, rect: Rect, p: Pos2, scroll: f32) {
        let before = self.focus_point(rect, p);
        self.zoom(scroll);
        if let (Some(b), Some(a)) = (before, self.focus_point(rect, p)) {
            self.target = add(self.target, sub(b, a));
        }
    }
    pub fn look(&mut self, yaw: f32, pitch: f32) {
        self.yaw = yaw;
        self.pitch = pitch.clamp(-MAX_PITCH, MAX_PITCH);
    }
    /// Fit a sphere of `radius` around `centre` into the view.
    pub fn frame(&mut self, centre: V3, radius: f32) {
        self.target = centre;
        self.dist = (radius.max(1.0) / TAN_HALF_FOV * 1.35).clamp(4.0, 4000.0);
    }
    /// The ray through a point of the view: ndc in -1..1 (y up).
    pub fn ray(&self, ndc: [f32; 2], aspect: f32) -> (V3, V3) {
        let (right, up, fwd) = self.basis();
        let eye = self.eye();
        if self.ortho {
            let h = self.half_height();
            let o = add(eye, add(mul(right, ndc[0] * aspect * h), mul(up, ndc[1] * h)));
            (o, fwd)
        } else {
            let d = add(fwd, add(mul(right, ndc[0] * aspect * TAN_HALF_FOV), mul(up, ndc[1] * TAN_HALF_FOV)));
            (eye, norm(d))
        }
    }
    /// The ray under a screen position inside `rect`.
    pub fn ray_at(&self, rect: Rect, p: Pos2) -> (V3, V3) {
        let w = rect.width().max(1.0);
        let h = rect.height().max(1.0);
        let nx = (p.x - rect.min.x) / w * 2.0 - 1.0;
        let ny = 1.0 - (p.y - rect.min.y) / h * 2.0;
        self.ray([nx, ny], w / h)
    }
    /// Where a world point lands on screen (None behind the camera).
    pub fn project(&self, rect: Rect, p: V3) -> Option<Pos2> {
        let (right, up, fwd) = self.basis();
        let v = sub(p, self.eye());
        let w = rect.width().max(1.0);
        let h = rect.height().max(1.0);
        let aspect = w / h;
        let (nx, ny) = if self.ortho {
            let half = self.half_height();
            (dot(v, right) / (aspect * half), dot(v, up) / half)
        } else {
            let z = dot(v, fwd);
            if z < 0.5 {
                return None;
            }
            (dot(v, right) / (z * TAN_HALF_FOV * aspect), dot(v, up) / (z * TAN_HALF_FOV))
        };
        if !nx.is_finite() || !ny.is_finite() {
            return None;
        }
        Some(pos2(rect.min.x + (nx * 0.5 + 0.5) * w, rect.min.y + (0.5 - ny * 0.5) * h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_and_ray_agree() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), egui::vec2(800.0, 500.0));
        for ortho in [false, true] {
            let c = Camera { ortho, ..Camera::default() };
            let p = [12.0, -7.0, 30.0];
            let s = c.project(rect, p).expect("in front");
            let (o, d) = c.ray_at(rect, s);
            // the ray through the projected point passes through the point
            let t = dot(sub(p, o), d);
            let q = add(o, mul(d, t));
            let e = sub(q, p);
            assert!(dot(e, e).sqrt() < 0.05, "ortho={ortho} miss {e:?}");
        }
    }

    #[test]
    fn wheel_zoom_keeps_the_point_under_the_cursor_in_both_projections() {
        let rect = Rect::from_min_size(pos2(30.0, 50.0), egui::vec2(980.0, 610.0));
        let mouse = pos2(790.0, 225.0);
        for ortho in [false, true] {
            let mut cam = Camera { ortho, ..Camera::default() };
            let anchor = cam.focus_point(rect, mouse).expect("cursor has a focus point");
            for amount in [120.0, 210.0, -310.0, -120.0] {
                cam.zoom_at(rect, mouse, amount);
                let after = cam.project(rect, anchor).expect("anchor remains visible");
                assert!((after - mouse).length() < 0.15, "ortho={ortho} anchor moved: {after:?}");
            }
        }
    }

    #[test]
    fn bad_or_large_pointer_events_do_not_jump_the_camera() {
        let mut cam = Camera::default();
        let before = cam;
        cam.orbit(f32::INFINITY, 20.0);
        cam.pan(f32::NAN, 10.0, 600.0);
        cam.zoom(f32::NAN);
        assert_eq!(cam.target, before.target);
        assert_eq!(cam.yaw, before.yaw);
        assert_eq!(cam.dist, before.dist);
        cam.orbit(100_000.0, -100_000.0);
        assert!((cam.yaw - before.yaw).abs() <= MAX_DRAG_STEP * ORBIT_RAD_PER_POINT + 1e-4);
        cam.zoom(100_000.0);
        assert!(cam.dist >= 4.0 && cam.dist.is_finite());
    }

    #[test]
    fn the_front_view_looks_along_plus_y_with_x_to_the_right() {
        let mut c = Camera::default();
        c.look(0.0, 0.0);
        let (right, up, fwd) = c.basis();
        assert!((fwd[1] - 1.0).abs() < 1e-5 && (right[0] - 1.0).abs() < 1e-5 && (up[2] - 1.0).abs() < 1e-5);
    }
}
