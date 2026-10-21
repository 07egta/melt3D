//! Exact, live sketch dimensions. Draft edits are transient; the finalized outline is stored
//! in the existing Shape/Scene format, so old project files remain compatible.
use crate::sketch;
use crate::sketch_ui::Pen;

#[derive(Clone, Copy, Debug)]
pub enum Target {
    Rectangle { anchor: [f32; 2], edge: [f32; 2] },
    Circle { center: [f32; 2], edge: [f32; 2] },
    Segment { index: usize, start: [f32; 2] },
    Point { index: usize },
}

/// Edits either the completed outline (rectangle/circle) or the most recent clicked vertex.
/// Only accepted, finite and bounded values reach the geometry.
#[derive(Clone, Debug)]
pub struct Dimensions {
    pub target: Target,
    pub fields: [String; 2],
    pub selected: usize,
    pub editing: bool,
    pub needs_focus: bool,
    pub error: Option<String>,
    /// True for a drawn loft top instead of the main outline.
    pub top_profile: bool,
    /// The point was added by Tab for this edit: Esc takes it away again.
    pub provisional: bool,
    before_fields: [String; 2],
    before_profile: Vec<[f32; 2]>,
    before_point: Option<[f32; 2]>,
}

fn number(v: f32) -> String {
    // Avoid rounding a typed value when it is copied into the other radius/diameter field.
    if v == 0.0 { "0".into() } else { v.to_string() }
}

fn finite_point(p: [f32; 2]) -> bool {
    p.iter().all(|v| v.is_finite() && v.abs() <= sketch::COORD_MAX)
}

fn sign(v: f32) -> f32 {
    if v < 0.0 { -1.0 } else { 1.0 }
}

impl Dimensions {
    pub fn for_shape(pen: Pen, a: [f32; 2], b: [f32; 2], profile: &[[f32; 2]], top: bool) -> Option<Self> {
        let (target, fields) = match pen {
            Pen::Rect => (Target::Rectangle { anchor: a, edge: b }, [number((b[0] - a[0]).abs()), number((b[1] - a[1]).abs())]),
            Pen::Circle => {
                let radius = (b[0] - a[0]).hypot(b[1] - a[1]);
                (Target::Circle { center: a, edge: b }, [number(radius), number(radius * 2.0)])
            }
            _ => return None,
        };
        Some(Self {
            target,
            before_fields: fields.clone(),
            fields,
            selected: 0,
            editing: true,
            needs_focus: true,
            error: None,
            top_profile: top,
            provisional: false,
            before_profile: profile.to_vec(),
            before_point: None,
        })
    }

    pub fn for_last_point(points: &[[f32; 2]], pen: Pen) -> Option<Self> {
        let index = points.len().checked_sub(1)?;
        let last = *points.get(index)?;
        let (target, fields) = if pen == Pen::Line && index > 0 {
            let start = *points.get(index - 1)?;
            let dx = last[0] - start[0];
            let dy = last[1] - start[1];
            (Target::Segment { index, start }, [number(dx.hypot(dy)), number(dy.atan2(dx).to_degrees())])
        } else {
            (Target::Point { index }, [number(last[0]), number(last[1])])
        };
        Some(Self {
            target,
            before_fields: fields.clone(),
            fields,
            selected: 0,
            editing: true,
            needs_focus: true,
            error: None,
            top_profile: false,
            provisional: false,
            before_profile: Vec::new(),
            before_point: Some(last),
        })
    }

    /// Length and angle of the segment that ends at the last point (a polygon or a line).
    pub fn for_new_segment(points: &[[f32; 2]]) -> Option<Self> {
        let index = points.len().checked_sub(1)?;
        if index == 0 {
            return None;
        }
        let last = *points.get(index)?;
        let start = *points.get(index - 1)?;
        let (dx, dy) = (last[0] - start[0], last[1] - start[1]);
        let fields = [number(dx.hypot(dy)), number(dy.atan2(dx).to_degrees())];
        Some(Self {
            target: Target::Segment { index, start },
            before_fields: fields.clone(),
            fields,
            selected: 0,
            editing: true,
            needs_focus: true,
            error: None,
            top_profile: false,
            provisional: true,
            before_profile: Vec::new(),
            before_point: Some(last),
        })
    }

    pub fn is_profile(&self) -> bool {
        matches!(self.target, Target::Rectangle { .. } | Target::Circle { .. })
    }

    pub fn labels(&self) -> [&'static str; 2] {
        match self.target {
            Target::Rectangle { .. } => ["Width", "Height"],
            Target::Circle { .. } => ["Radius", "Diameter"],
            Target::Segment { .. } => ["Length", "Angle"],
            Target::Point { .. } => ["X", "Y"],
        }
    }

    pub fn units(&self) -> [&'static str; 2] {
        match self.target {
            Target::Segment { .. } => ["mm", "°"],
            _ => ["mm", "mm"],
        }
    }

    pub fn readout(&self) -> String {
        let labels = self.labels();
        let units = self.units();
        format!("{} {}{}  /  {} {}{}", labels[0], self.fields[0], units[0], labels[1], self.fields[1], units[1])
    }

    pub fn position(&self, points: &[[f32; 2]]) -> Option<[f32; 2]> {
        match self.target {
            Target::Rectangle { anchor, edge } => {
                let w = self.fields[0].parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or((edge[0] - anchor[0]).abs());
                let h = self.fields[1].parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or((edge[1] - anchor[1]).abs());
                Some([anchor[0] + sign(edge[0] - anchor[0]) * w, anchor[1] + sign(edge[1] - anchor[1]) * h])
            }
            Target::Circle { center, edge } => {
                let old = (edge[0] - center[0]).hypot(edge[1] - center[1]).max(1e-6);
                let r = self.fields[0].parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(old);
                Some([center[0] + (edge[0] - center[0]) / old * r, center[1] + (edge[1] - center[1]) / old * r])
            }
            Target::Segment { index, .. } | Target::Point { index } => points.get(index).copied(),
        }
    }

    fn parse(&self, index: usize) -> Result<f32, String> {
        let Some(value) = self.fields.get(index) else { return Err("Input field not found".into()) };
        if value.len() > 32 {
            return Err("Enter a number of 32 characters or fewer".into());
        }
        let Ok(n) = value.trim().parse::<f32>() else {
            return Err("Enter a valid number".into());
        };
        if !n.is_finite() {
            return Err("Enter a finite number".into());
        }
        Ok(n)
    }

    /// Apply on each TextEdit change, never only after Enter. Invalid intermediate strings
    /// leave the last valid geometry visible and disable confirmation.
    pub fn update(&mut self, points: &mut [[f32; 2]], profile: &mut Vec<[f32; 2]>, changed: usize, prevent_cross: bool) {
        type DimensionChange = (Option<Vec<[f32; 2]>>, Option<(usize, [f32; 2])>, Option<String>);
        let result: Result<DimensionChange, String> = (|| match self.target {
            Target::Rectangle { anchor, edge } => {
                let (w, h) = (self.parse(0)?, self.parse(1)?);
                if w <= sketch::MIN_SIDE || h <= sketch::MIN_SIDE {
                    return Err("Width and height must be greater than 0.5 mm".into());
                }
                let b = [anchor[0] + sign(edge[0] - anchor[0]) * w, anchor[1] + sign(edge[1] - anchor[1]) * h];
                if !finite_point(anchor) || !finite_point(b) {
                    return Err("Coordinate is outside the sketch area".into());
                }
                let p = sketch::rect_profile(anchor, b).map_err(sketch::SketchError::message)?;
                Ok((Some(p), None, None))
            }
            Target::Circle { center, edge } => {
                let input = self.parse(changed.min(1))?;
                let radius = if changed == 0 { input } else { input / 2.0 };
                if radius <= sketch::MIN_SIDE {
                    return Err("Radius must be greater than 0.5 mm".into());
                }
                let old = (edge[0] - center[0]).hypot(edge[1] - center[1]);
                if old < sketch::MIN_SIDE {
                    return Err("Can't determine circle orientation".into());
                }
                let b = [center[0] + (edge[0] - center[0]) * radius / old, center[1] + (edge[1] - center[1]) * radius / old];
                if !finite_point(center) || !finite_point(b) {
                    return Err("Coordinate is outside the sketch area".into());
                }
                let p = sketch::circle_profile(center, b).map_err(sketch::SketchError::message)?;
                if !p.iter().copied().all(finite_point) {
                    return Err("Circle is outside the sketch area".into());
                }
                let other = if changed == 0 { number(radius * 2.0) } else { number(radius) };
                Ok((Some(p), None, Some(other)))
            }
            Target::Segment { index, start } => {
                let length = self.parse(0)?;
                let angle = self.parse(1)?;
                if length <= sketch::MIN_SIDE || angle.abs() > 36000.0 {
                    return Err("Length must be greater than 0.5 mm and angle within ±36000°".into());
                }
                let rad = angle.to_radians();
                let b = [start[0] + length * rad.cos(), start[1] + length * rad.sin()];
                if !finite_point(b) || points.get(index).is_none() {
                    return Err("Point is outside the sketch area".into());
                }
                // the same check as placing the point with the mouse
                if prevent_cross && index >= 2 && sketch::crosses_polyline(&points[..index], start, b, false) {
                    return Err("The outline crosses at this point".into());
                }
                Ok((None, Some((index, b)), None))
            }
            Target::Point { index } => {
                let b = [self.parse(0)?, self.parse(1)?];
                if !finite_point(b) || points.get(index).is_none() {
                    return Err("X and Y must be within ±2000 mm".into());
                }
                if let Some(previous) = index.checked_sub(1).and_then(|i| points.get(i)) {
                    if (previous[0] - b[0]).hypot(previous[1] - b[1]) < 1e-3 {
                        return Err("Place it apart from the previous point".into());
                    }
                    if prevent_cross && index >= 2 && sketch::crosses_polyline(&points[..index], *previous, b, false) {
                        return Err("The outline crosses at this point".into());
                    }
                }
                Ok((None, Some((index, b)), None))
            }
        })();
        match result {
            Ok((new_profile, new_point, synced)) => {
                if let Some(p) = new_profile {
                    *profile = p;
                }
                if let Some((index, p)) = new_point
                    && let Some(current) = points.get_mut(index)
                {
                    *current = p;
                }
                if let Some(value) = synced {
                    self.fields[1 - changed.min(1)] = value;
                }
                self.error = None;
            }
            Err(message) => self.error = Some(message),
        }
    }

    /// Esc cancels the in-progress changes and restores the state at the last Enter.
    pub fn cancel(&mut self, points: &mut Vec<[f32; 2]>, profile: &mut Vec<[f32; 2]>) {
        if self.provisional {
            points.pop();
        } else if self.is_profile() {
            profile.clone_from(&self.before_profile);
        } else if let Some(old) = self.before_point
            && let Target::Segment { index, .. } | Target::Point { index } = self.target
            && let Some(p) = points.get_mut(index)
        {
            *p = old;
        }
        self.fields.clone_from(&self.before_fields);
        self.editing = false;
        self.needs_focus = false;
        self.error = None;
    }

    /// Enter accepts the last valid geometry as the baseline for a possible later Tab edit.
    pub fn accept(&mut self, points: &[[f32; 2]], profile: &[[f32; 2]]) -> bool {
        if self.error.is_some() {
            return false;
        }
        match &mut self.target {
            Target::Rectangle { anchor, edge } => {
                if let (Ok(w), Ok(h)) = (self.fields[0].parse::<f32>(), self.fields[1].parse::<f32>()) {
                    edge[0] = anchor[0] + sign(edge[0] - anchor[0]) * w;
                    edge[1] = anchor[1] + sign(edge[1] - anchor[1]) * h;
                }
                self.before_profile = profile.to_vec();
            }
            Target::Circle { center, edge } => {
                let old = (edge[0] - center[0]).hypot(edge[1] - center[1]);
                if let Ok(radius) = self.fields[0].parse::<f32>()
                    && old > 0.0
                {
                    edge[0] = center[0] + (edge[0] - center[0]) / old * radius;
                    edge[1] = center[1] + (edge[1] - center[1]) / old * radius;
                }
                self.before_profile = profile.to_vec();
            }
            Target::Segment { index, .. } | Target::Point { index } => self.before_point = points.get(*index).copied(),
        }
        self.before_fields.clone_from(&self.fields);
        self.editing = false;
        self.needs_focus = false;
        true
    }

    pub fn begin_again(&mut self) {
        self.editing = true;
        self.selected = 0;
        self.needs_focus = true;
        self.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approximately(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn rectangle_edits_both_dimensions_in_the_original_drag_quadrant() {
        let old = sketch::rect_profile([10.0, 20.0], [-15.0, 8.0]).expect("rectangle");
        let mut d = Dimensions::for_shape(Pen::Rect, [10.0, 20.0], [-15.0, 8.0], &old, false).expect("fields");
        let mut p = old.clone();
        d.fields = ["36.25".into(), "17.5".into()];
        d.update(&mut [], &mut p, 1, false);
        assert!(d.error.is_none());
        let (min, max) = sketch::bbox(&p).expect("box");
        assert!(approximately(min[0], -26.25) && approximately(max[1], 20.0));
        assert!(approximately(max[0] - min[0], 36.25) && approximately(max[1] - min[1], 17.5));
        d.cancel(&mut vec![], &mut p);
        assert_eq!(p, old, "Esc restores the exact original outline");
    }

    #[test]
    fn circle_radius_and_diameter_are_consistent_and_live() {
        let old = sketch::circle_profile([0.0, 0.0], [4.0, 0.0]).expect("circle");
        let mut d = Dimensions::for_shape(Pen::Circle, [0.0, 0.0], [4.0, 0.0], &old, false).expect("fields");
        let mut p = old;
        d.fields[1] = "30".into();
        d.update(&mut [], &mut p, 1, false);
        assert_eq!(d.fields[0], "15");
        assert!(approximately(p[0][0].hypot(p[0][1]), 15.0));
        d.fields[0] = "8".into();
        d.update(&mut [], &mut p, 0, false);
        assert_eq!(d.fields[1], "16");
        assert!(approximately(p[0][0].hypot(p[0][1]), 8.0));
    }

    #[test]
    fn line_uses_degrees_from_positive_x() {
        let mut pts = vec![[2.0, 3.0], [4.0, 3.0]];
        let mut d = Dimensions::for_last_point(&pts, Pen::Line).expect("line");
        let mut profile = vec![];
        d.fields = ["20".into(), "90".into()];
        d.update(&mut pts, &mut profile, 1, false);
        assert!(d.error.is_none());
        assert!(approximately(pts[1][0], 2.0) && approximately(pts[1][1], 23.0));
        assert!(d.accept(&pts, &profile));
        d.begin_again();
        d.fields[1] = "-90".into();
        d.update(&mut pts, &mut profile, 1, false);
        assert!(approximately(pts[1][1], -17.0));
        d.cancel(&mut pts, &mut profile);
        assert!(approximately(pts[1][1], 23.0));
    }

    #[test]
    fn latest_vertex_x_y_update_live_and_invalid_numbers_do_not_change_it() {
        let mut pts = vec![[0.0, 0.0], [20.0, 0.0], [15.0, 12.0]];
        let mut d = Dimensions::for_last_point(&pts, Pen::Polygon).expect("point");
        let mut profile = vec![];
        d.fields = ["13.25".into(), "18.75".into()];
        d.update(&mut pts, &mut profile, 0, true);
        assert_eq!(pts[2], [13.25, 18.75]);
        d.fields[0] = "NaN".into();
        d.update(&mut pts, &mut profile, 0, true);
        assert!(d.error.is_some());
        assert_eq!(pts[2], [13.25, 18.75]);
        assert!(!d.accept(&pts, &profile));
        d.cancel(&mut pts, &mut profile);
        assert_eq!(pts[2], [15.0, 12.0]);
    }

    #[test]
    fn a_typed_length_and_angle_may_not_cross_the_outline_either() {
        // three corners placed, the fourth is the segment being typed
        let mut pts = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [10.0, 20.0]];
        let mut d = Dimensions::for_new_segment(&pts).expect("segment");
        let mut profile = vec![];
        let before = pts[3];
        // from (20, 20) down and to the left, through the first side
        d.fields = ["40".into(), "-120".into()];
        d.update(&mut pts, &mut profile, 1, true);
        assert!(d.error.is_some(), "crossing is refused");
        assert_eq!(pts[3], before, "the draft stays as it was");
        // the same numbers are fine where nothing is crossed (other pens do not check)
        d.update(&mut pts, &mut profile, 1, false);
        assert!(d.error.is_none());
        d.fields = ["5".into(), "90".into()];
        d.update(&mut pts, &mut profile, 1, true);
        assert!(d.error.is_none(), "{:?}", d.error);
    }

    #[test]
    fn bounds_and_self_intersection_are_rejected_without_destroying_preview() {
        let old = sketch::rect_profile([0.0, 0.0], [10.0, 5.0]).expect("rectangle");
        let mut d = Dimensions::for_shape(Pen::Rect, [0.0, 0.0], [10.0, 5.0], &old, false).expect("fields");
        let mut profile = old.clone();
        d.fields[0] = "30000".into();
        d.update(&mut [], &mut profile, 0, false);
        assert_eq!(profile, old);
        assert!(d.error.is_some());

        let mut pts = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]];
        let mut d = Dimensions::for_last_point(&pts, Pen::Polygon).expect("point");
        d.fields = ["10".into(), "-10".into()];
        d.update(&mut pts, &mut vec![], 1, true);
        assert!(d.error.is_some());
        assert_eq!(pts[3], [0.0, 20.0]);
    }
}

#[cfg(test)]
mod tab_segment_tests {
    use super::*;

    #[test]
    fn a_tab_segment_types_length_and_angle_and_esc_takes_it_away() {
        let mut points = vec![[0.0, 0.0], [10.0, 0.0]];
        let Some(mut d) = Dimensions::for_new_segment(&points) else { panic!("segment") };
        assert_eq!(d.labels(), ["Length", "Angle"]);
        d.fields = ["20".into(), "90".into()];
        let mut profile = Vec::new();
        d.update(&mut points, &mut profile, 0, false);
        let p = points[1];
        assert!(p[0].abs() < 1e-3 && (p[1] - 20.0).abs() < 1e-3, "{p:?}");
        assert!(d.accept(&points, &profile));
        // a second one, cancelled: the point goes away again
        points.push([5.0, 20.0]);
        let Some(mut d2) = Dimensions::for_new_segment(&points) else { panic!("segment") };
        d2.cancel(&mut points, &mut profile);
        assert_eq!(points.len(), 2);
        assert!(Dimensions::for_new_segment(&[[0.0, 0.0]]).is_none(), "one point has no segment");
    }
}
