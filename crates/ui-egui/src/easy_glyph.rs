//! Small shapes for the easy layout: what a picture says faster than a word does. Planes, the
//! four display styles and the four things a click can pick are all drawn here, in code, from
//! the theme's colours.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

use crate::theme::Tokens;

/// A point of the unit cube, on screen: x runs right and down, y left and down, z up.
struct Iso {
    c: Pos2,
    s: f32,
}

impl Iso {
    fn at(&self, p: [f32; 3]) -> Pos2 {
        let (x, y, z) = (p[0] - 0.5, p[1] - 0.5, p[2] - 0.5);
        self.c + vec2((x - y) * 0.866, (x + y) * 0.5 - z) * self.s
    }
}

fn iso(rect: Rect) -> Iso {
    Iso { c: rect.center(), s: rect.width().min(rect.height()) * 0.5 }
}

fn dashed(painter: &Painter, pts: &[Pos2], stroke: Stroke) {
    let mut closed = pts.to_vec();
    if let Some(first) = pts.first() {
        closed.push(*first);
    }
    painter.extend(Shape::dashed_line(&closed, stroke, 3.0, 2.5));
}

/// A plane (`"XY"`, `"YZ"` or `"XZ"`): a dashed sheet with its two axes drawn in their colours.
pub fn plane(painter: &Painter, rect: Rect, which: &str, ink: Color32) {
    let t = Tokens::get();
    let g = iso(rect.shrink(1.0));
    // The sheet's corners in the unit cube, and the two axes from its first corner.
    let (corners, a, b, ca, cb): ([[f32; 3]; 4], [f32; 3], [f32; 3], Color32, Color32) = match which {
        "YZ" => ([[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [0.0, 0.0, 1.0]], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], t.axis_y, t.axis_z),
        "XZ" => ([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], t.axis_x, t.axis_z),
        _ => ([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], t.axis_x, t.axis_y),
    };
    let pts: Vec<Pos2> = corners.iter().map(|p| g.at(*p)).collect();
    painter.add(Shape::convex_polygon(pts.clone(), t.accent_soft, Stroke::NONE));
    dashed(painter, &pts, Stroke::new(1.0, ink));
    let o = g.at(corners[0]);
    painter.line_segment([o, g.at(a)], Stroke::new(1.8, ca));
    painter.line_segment([o, g.at(b)], Stroke::new(1.8, cb));
}

const CUBE_FACES: [[[f32; 3]; 4]; 3] = [
    // top, left, right
    [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
    [[0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
    [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 1.0]],
];

/// The edges of the cube that face the viewer, and those hidden behind it.
const FRONT_EDGES: [([f32; 3], [f32; 3]); 9] = [
    ([0.0, 0.0, 1.0], [1.0, 0.0, 1.0]),
    ([1.0, 0.0, 1.0], [1.0, 1.0, 1.0]),
    ([1.0, 1.0, 1.0], [0.0, 1.0, 1.0]),
    ([0.0, 1.0, 1.0], [0.0, 0.0, 1.0]),
    ([1.0, 0.0, 0.0], [1.0, 1.0, 0.0]),
    ([1.0, 1.0, 0.0], [0.0, 1.0, 0.0]),
    ([1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
    ([1.0, 1.0, 0.0], [1.0, 1.0, 1.0]),
    ([0.0, 1.0, 0.0], [0.0, 1.0, 1.0]),
];
const HIDDEN_EDGES: [([f32; 3], [f32; 3]); 3] =
    [([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]), ([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]), ([0.0, 0.0, 0.0], [0.0, 0.0, 1.0])];

/// How the model is drawn: 0 shaded with edges, 1 shaded, 2 wireframe, 3 hidden edges shown dashed.
pub fn display_style(painter: &Painter, rect: Rect, style: u8, ink: Color32) {
    let g = iso(rect.shrink(2.0));
    let shade = [ink.gamma_multiply(0.55), ink.gamma_multiply(0.30), ink.gamma_multiply(0.42)];
    if style <= 1 {
        for (face, fill) in CUBE_FACES.iter().zip(shade) {
            let pts: Vec<Pos2> = face.iter().map(|p| g.at(*p)).collect();
            painter.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
        }
    }
    let line = Stroke::new(1.1, ink);
    if style != 1 {
        for (a, b) in FRONT_EDGES {
            painter.line_segment([g.at(a), g.at(b)], line);
        }
    }
    if style >= 2 {
        for (a, b) in HIDDEN_EDGES {
            if style == 3 {
                painter.extend(Shape::dashed_line(&[g.at(a), g.at(b)], Stroke::new(1.0, ink.gamma_multiply(0.8)), 2.0, 2.0));
            } else {
                painter.line_segment([g.at(a), g.at(b)], Stroke::new(1.0, ink.gamma_multiply(0.7)));
            }
        }
    }
}

/// What a click picks: `"body"`, `"faces"`, `"edges"` or `"vertices"`. The part that is picked is
/// drawn in `hot`, the rest of the cube in `ink`.
pub fn pick(painter: &Painter, rect: Rect, kind: &str, ink: Color32, hot: Color32) {
    let g = iso(rect.shrink(2.0));
    let faint = Stroke::new(1.1, ink);
    match kind {
        "body" => {
            for (face, k) in CUBE_FACES.iter().zip([0.9, 0.5, 0.7]) {
                let pts: Vec<Pos2> = face.iter().map(|p| g.at(*p)).collect();
                painter.add(Shape::convex_polygon(pts, hot.gamma_multiply(k), faint));
            }
        }
        "faces" => {
            for (a, b) in FRONT_EDGES {
                painter.line_segment([g.at(a), g.at(b)], faint);
            }
            let pts: Vec<Pos2> = CUBE_FACES[0].iter().map(|p| g.at(*p)).collect();
            painter.add(Shape::convex_polygon(pts, hot.gamma_multiply(0.85), faint));
        }
        "edges" => {
            for (a, b) in FRONT_EDGES {
                painter.line_segment([g.at(a), g.at(b)], faint);
            }
            painter.line_segment([g.at([1.0, 0.0, 1.0]), g.at([1.0, 1.0, 1.0])], Stroke::new(2.6, hot));
        }
        _ => {
            for (a, b) in FRONT_EDGES {
                painter.line_segment([g.at(a), g.at(b)], faint);
            }
            painter.circle_filled(g.at([1.0, 1.0, 1.0]), 3.2, hot);
        }
    }
}

/// A key cap: the text inside a small outlined box, right edge at `right`, centred on `y`.
/// Returns the box's left edge.
pub fn key_cap(painter: &Painter, right: f32, y: f32, text: &str, ink: Color32, edge: Color32) -> f32 {
    let galley = painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(10.5), ink);
    let size = galley.size() + vec2(10.0, 4.0);
    let r = Rect::from_min_size(pos2(right - size.x, y - size.y / 2.0), size);
    painter.rect_stroke(r, 4.0, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    painter.galley(pos2(r.left() + 5.0, r.top() + 2.0), galley, ink);
    r.left()
}

/// An icon for a command that has none of its own: the kind of thing it belongs to.
pub fn fallback_icon(id: &str) -> &'static str {
    let area = id.split('.').next().unwrap_or("");
    match area {
        "sketch" => "sketch",
        "solid" => "box",
        "surface" => "surface_loft",
        "inspect" => "measure",
        "file" | "doc" => "open",
        "construct" => "plane",
        "component" | "occurrence" => "component",
        "joint" | "motion" => "joint",
        "sheet" => "flange",
        "plastic" => "plastic_rule",
        "mesh" => "import",
        "config" => "configuration_table",
        "face" => "presspull",
        "view" | "camera" => "eye",
        "timeline" | "edit" | "delete" => "delete",
        "materials" | "appearance" => "appearance",
        _ => "settings",
    }
}
