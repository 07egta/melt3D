//! The components: what each node does. Each one takes one item per input and returns one list
//! per output; the graph matches lists and trees around it.

use std::sync::Arc;

use solidcraft_geom::Vec3;
use solidcraft_slice::{Tri, gcode, slice};

use crate::{MAX_ITEMS, Poly, Value, print_config};

/// One input of a component.
#[derive(Debug)]
pub struct Input {
    pub name: &'static str,
    /// Receives a whole branch as one [`Value::List`] instead of one item at a time.
    pub gather: bool,
}

#[derive(Debug)]
pub struct Component {
    pub name: &'static str,
    /// The group it is listed under.
    pub group: &'static str,
    pub inputs: &'static [Input],
    pub outputs: &'static [&'static str],
    /// The value each input holds until a wire replaces it.
    pub defaults: fn() -> Vec<Value>,
    pub run: fn(&[Value]) -> Result<Vec<Vec<Value>>, String>,
}

type Run = Result<Vec<Vec<Value>>, String>;

const fn i(name: &'static str) -> Input {
    Input { name, gather: false }
}

const fn all(name: &'static str) -> Input {
    Input { name, gather: true }
}

fn n(x: f64) -> Value {
    Value::Number(x)
}

/// Every component, sorted by group and then by name in the UI.
pub const COMPONENTS: &[Component] = &[
    Component { name: "Number", group: "Math", inputs: &[i("value")], outputs: &["value"], defaults: || vec![n(1.0)], run: run_number },
    Component {
        name: "Add",
        group: "Math",
        inputs: &[i("a"), i("b")],
        outputs: &["result"],
        defaults: || vec![n(0.0), n(0.0)],
        run: |a| arith(a, |x, y| Ok(x + y)),
    },
    Component {
        name: "Subtract",
        group: "Math",
        inputs: &[i("a"), i("b")],
        outputs: &["result"],
        defaults: || vec![n(0.0), n(0.0)],
        run: |a| arith(a, |x, y| Ok(x - y)),
    },
    Component {
        name: "Multiply",
        group: "Math",
        inputs: &[i("a"), i("b")],
        outputs: &["result"],
        defaults: || vec![n(1.0), n(1.0)],
        run: |a| arith(a, |x, y| Ok(x * y)),
    },
    Component {
        name: "Divide",
        group: "Math",
        inputs: &[i("a"), i("b")],
        outputs: &["result"],
        defaults: || vec![n(1.0), n(1.0)],
        run: |a| arith(a, |x, y| if y == 0.0 { Err("cannot divide by zero".to_string()) } else { Ok(x / y) }),
    },
    Component {
        name: "Series",
        group: "Math",
        inputs: &[i("start"), i("step"), i("count")],
        outputs: &["list"],
        defaults: || vec![n(0.0), n(1.0), n(5.0)],
        run: run_series,
    },
    Component {
        name: "Point",
        group: "Curves",
        inputs: &[i("x"), i("y"), i("z")],
        outputs: &["point"],
        defaults: || vec![n(0.0), n(0.0), n(0.0)],
        run: run_point,
    },
    Component {
        name: "Circle",
        group: "Curves",
        inputs: &[i("radius"), i("segments")],
        outputs: &["curve"],
        defaults: || vec![n(10.0), n(48.0)],
        run: run_circle,
    },
    Component {
        name: "Rectangle",
        group: "Curves",
        inputs: &[i("width"), i("depth")],
        outputs: &["curve"],
        defaults: || vec![n(20.0), n(20.0)],
        run: run_rectangle,
    },
    Component {
        name: "Move",
        group: "Transform",
        inputs: &[i("shape"), i("x"), i("y"), i("z")],
        outputs: &["shape"],
        defaults: || vec![n(0.0), n(0.0), n(0.0), n(0.0)],
        run: run_move,
    },
    Component {
        name: "Rotate",
        group: "Transform",
        inputs: &[i("shape"), i("degrees")],
        outputs: &["shape"],
        defaults: || vec![n(0.0), n(0.0)],
        run: run_rotate,
    },
    Component {
        name: "Linear Pattern",
        group: "Pattern",
        inputs: &[i("shape"), i("count"), i("dx"), i("dy"), i("dz")],
        outputs: &["shapes"],
        defaults: || vec![n(0.0), n(3.0), n(25.0), n(0.0), n(0.0)],
        run: run_linear,
    },
    Component {
        name: "Circular Pattern",
        group: "Pattern",
        inputs: &[i("shape"), i("count")],
        outputs: &["shapes"],
        defaults: || vec![n(0.0), n(6.0)],
        run: run_polar,
    },
    Component {
        name: "Extrude",
        group: "Solid",
        inputs: &[i("curve"), i("height")],
        outputs: &["solid"],
        defaults: || vec![n(0.0), n(5.0)],
        run: run_extrude,
    },
    Component { name: "Merge Solids", group: "Solid", inputs: &[all("solids")], outputs: &["solid"], defaults: || vec![n(0.0)], run: run_merge },
    Component {
        name: "Slice",
        group: "Print",
        inputs: &[i("solid"), i("layer height")],
        outputs: &["layers"],
        defaults: || vec![n(0.0), n(0.2)],
        run: run_slice,
    },
    Component {
        name: "G-code",
        group: "Print",
        inputs: &[i("layers"), i("line width"), i("nozzle temp"), i("bed temp"), i("walls"), i("infill spacing")],
        outputs: &["text"],
        defaults: || vec![n(0.0), n(0.42), n(220.0), n(60.0), n(2.0), n(3.0)],
        run: run_gcode,
    },
];

pub fn component(name: &str) -> Option<&'static Component> {
    COMPONENTS.iter().find(|c| c.name == name)
}

fn arg(a: &[Value], k: usize) -> Result<&Value, String> {
    a.get(k).ok_or_else(|| "an input is missing".to_string())
}

fn num(a: &[Value], k: usize) -> Result<f64, String> {
    arg(a, k)?.number()
}

fn run_number(a: &[Value]) -> Run {
    Ok(vec![vec![n(num(a, 0)?)]])
}

fn arith(a: &[Value], f: impl Fn(f64, f64) -> Result<f64, String>) -> Run {
    let r = f(num(a, 0)?, num(a, 1)?)?;
    if !r.is_finite() {
        return Err("the result is not finite".into());
    }
    Ok(vec![vec![n(r)]])
}

fn run_series(a: &[Value]) -> Run {
    let (start, step, count) = (num(a, 0)?, num(a, 1)?, num(a, 2)?);
    if !(count >= 0.0) || count > MAX_ITEMS as f64 {
        return Err(format!("count must be between 0 and {MAX_ITEMS}"));
    }
    Ok(vec![(0..count as usize).map(|k| n(start + step * k as f64)).collect()])
}

fn run_point(a: &[Value]) -> Run {
    Ok(vec![vec![Value::Point(Vec3::new(num(a, 0)?, num(a, 1)?, num(a, 2)?))]])
}

fn run_circle(a: &[Value]) -> Run {
    let (r, seg) = (num(a, 0)?, num(a, 1)?);
    if !(r > 0.0) || r > 1e6 {
        return Err("radius must be more than 0".into());
    }
    if !(3.0..=512.0).contains(&seg) {
        return Err("segments must be between 3 and 512".into());
    }
    let seg = seg as usize;
    let pts = (0..seg)
        .map(|k| {
            let t = std::f64::consts::TAU * k as f64 / seg as f64;
            Vec3::new(r * t.cos(), r * t.sin(), 0.0)
        })
        .collect();
    Ok(vec![vec![Value::Poly(Poly { pts, closed: true })]])
}

fn run_rectangle(a: &[Value]) -> Run {
    let (w, d) = (num(a, 0)?, num(a, 1)?);
    if !(w > 0.0) || !(d > 0.0) || w > 1e6 || d > 1e6 {
        return Err("width and depth must be more than 0".into());
    }
    let (x, y) = (w * 0.5, d * 0.5);
    let pts = vec![Vec3::new(-x, -y, 0.0), Vec3::new(x, -y, 0.0), Vec3::new(x, y, 0.0), Vec3::new(-x, y, 0.0)];
    Ok(vec![vec![Value::Poly(Poly { pts, closed: true })]])
}

/// Applies `f` to every point of a curve or to a point.
fn map_shape(v: &Value, f: impl Fn(Vec3) -> Vec3) -> Result<Value, String> {
    match v {
        Value::Point(p) => Ok(Value::Point(f(*p))),
        Value::Poly(p) => Ok(Value::Poly(Poly { pts: p.pts.iter().map(|q| f(*q)).collect(), closed: p.closed })),
        _ => Err("expected a point or a curve".into()),
    }
}

fn shifted(p: Vec3, d: Vec3) -> Vec3 {
    Vec3::new(p.x + d.x, p.y + d.y, p.z + d.z)
}

fn turned(p: Vec3, degrees: f64) -> Vec3 {
    let (s, c) = degrees.to_radians().sin_cos();
    Vec3::new(p.x * c - p.y * s, p.x * s + p.y * c, p.z)
}

fn run_move(a: &[Value]) -> Run {
    let d = Vec3::new(num(a, 1)?, num(a, 2)?, num(a, 3)?);
    Ok(vec![vec![map_shape(arg(a, 0)?, |p| shifted(p, d))?]])
}

fn run_rotate(a: &[Value]) -> Run {
    let deg = num(a, 1)?;
    Ok(vec![vec![map_shape(arg(a, 0)?, |p| turned(p, deg))?]])
}

fn count(a: &[Value], k: usize) -> Result<usize, String> {
    let c = num(a, k)?;
    if !(1.0..=256.0).contains(&c) {
        return Err("count must be between 1 and 256".into());
    }
    Ok(c as usize)
}

fn run_linear(a: &[Value]) -> Run {
    let c = count(a, 1)?;
    let d = Vec3::new(num(a, 2)?, num(a, 3)?, num(a, 4)?);
    let shape = arg(a, 0)?;
    let mut out = Vec::with_capacity(c);
    for k in 0..c {
        let f = k as f64;
        let step = Vec3::new(d.x * f, d.y * f, d.z * f);
        out.push(map_shape(shape, |p| shifted(p, step))?);
    }
    Ok(vec![out])
}

fn run_polar(a: &[Value]) -> Run {
    let c = count(a, 1)?;
    let shape = arg(a, 0)?;
    let mut out = Vec::with_capacity(c);
    for k in 0..c {
        let deg = 360.0 * k as f64 / c as f64;
        out.push(map_shape(shape, |p| turned(p, deg))?);
    }
    Ok(vec![out])
}

/// A prism: the curve's ring raised by `height`, with fan-shaped caps (good for convex and
/// star-shaped outlines).
fn run_extrude(a: &[Value]) -> Run {
    let poly = arg(a, 0)?.poly()?;
    let h = num(a, 1)?;
    if !(h > 0.0) || h > 1e5 {
        return Err("height must be more than 0".into());
    }
    if !poly.closed || poly.pts.len() < 3 {
        return Err("extrude needs a closed curve".into());
    }
    let up = |p: Vec3| Vec3::new(p.x, p.y, p.z + h);
    let k = poly.pts.len();
    let mut tris: Vec<Tri> = Vec::with_capacity(k * 4);
    let (mut cx, mut cy, mut cz) = (0.0, 0.0, 0.0);
    for p in &poly.pts {
        cx += p.x;
        cy += p.y;
        cz += p.z;
    }
    let centre = Vec3::new(cx / k as f64, cy / k as f64, cz / k as f64);
    for (idx, p) in poly.pts.iter().enumerate() {
        let q = poly.pts.get((idx + 1) % k).copied().unwrap_or(*p);
        tris.push([*p, q, up(*p)]);
        tris.push([q, up(q), up(*p)]);
        tris.push([centre, q, *p]);
        tris.push([up(centre), up(*p), up(q)]);
    }
    Ok(vec![vec![Value::Solid(Arc::new(tris))]])
}

fn run_merge(a: &[Value]) -> Run {
    let Value::List(items) = arg(a, 0)? else { return Err("expected solids".into()) };
    let mut tris: Vec<Tri> = Vec::new();
    for v in items {
        match v {
            Value::Solid(s) => tris.extend(s.iter().copied()),
            _ => return Err("expected solids".into()),
        }
        if tris.len() > solidcraft_slice::MAX_TRIANGLES {
            return Err("too many triangles".into());
        }
    }
    Ok(vec![vec![Value::Solid(Arc::new(tris))]])
}

fn run_slice(a: &[Value]) -> Run {
    let Value::Solid(s) = arg(a, 0)? else { return Err("connect a solid".into()) };
    let sliced = slice(s, num(a, 1)?).map_err(|e| e.to_string())?;
    Ok(vec![vec![Value::Layers(Arc::new(sliced))]])
}

fn run_gcode(a: &[Value]) -> Run {
    let Value::Layers(l) = arg(a, 0)? else { return Err("connect sliced layers".into()) };
    let cfg = print_config(num(a, 1)?, num(a, 2)?, num(a, 3)?, num(a, 4)?, num(a, 5)?);
    let text = gcode(l, &cfg).map_err(|e| e.to_string())?;
    Ok(vec![vec![Value::Text(text)]])
}
