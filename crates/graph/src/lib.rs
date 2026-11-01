//! A node graph in the style of a visual algorithm editor: components joined by wires, data held
//! in trees, lists matched item by item. The components make curves, patterns and solids, slice
//! them and write G-code.
//!
//! Everything is plain data, so it is tested without a window. A component that fails marks only
//! its own node (and the nodes after it); the rest of the graph still runs. All counts are capped.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use solidcraft_geom::Vec3;
use solidcraft_slice::{PrintConfig, Sliced, Tri};

mod components;

pub use components::{COMPONENTS, Component, Input, component};

/// Most nodes in one graph.
pub const MAX_NODES: usize = 2_000;
/// Most items one component run may make, and most items one wire may carry.
pub const MAX_ITEMS: usize = 100_000;

/// A curve: a chain of points, closed or open.
#[derive(Clone, Debug, PartialEq)]
pub struct Poly {
    pub pts: Vec<Vec3>,
    pub closed: bool,
}

/// One piece of data on a wire.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Bool(bool),
    Text(String),
    Point(Vec3),
    Poly(Poly),
    Solid(Arc<Vec<Tri>>),
    Layers(Arc<Sliced>),
    /// A whole branch handed to a component that asked for it (see [`Input::gather`]).
    List(Vec<Value>),
}

impl Value {
    pub fn number(&self) -> Result<f64, String> {
        match self {
            Value::Number(n) if n.is_finite() => Ok(*n),
            Value::Number(_) => Err("a number is not finite".into()),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            _ => Err("expected a number".into()),
        }
    }
    pub fn poly(&self) -> Result<&Poly, String> {
        match self {
            Value::Poly(p) => Ok(p),
            _ => Err("expected a curve".into()),
        }
    }
    /// A short name for the kind of data, for the UI.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::Bool(_) => "yes/no",
            Value::Text(_) => "text",
            Value::Point(_) => "point",
            Value::Poly(_) => "curve",
            Value::Solid(_) => "solid",
            Value::Layers(_) => "layers",
            Value::List(_) => "list",
        }
    }
}

/// Data on a wire: lists of values filed under branch paths.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tree {
    pub branches: BTreeMap<Vec<u32>, Vec<Value>>,
}

impl Tree {
    pub fn single(v: Value) -> Tree {
        Tree { branches: BTreeMap::from([(Vec::new(), vec![v])]) }
    }
    pub fn item_count(&self) -> usize {
        self.branches.values().map(Vec::len).sum()
    }
    /// Every item in one branch.
    pub fn flatten(&self) -> Tree {
        let all: Vec<Value> = self.branches.values().flatten().cloned().collect();
        Tree { branches: BTreeMap::from([(Vec::new(), all)]) }
    }
    /// Every item in a branch of its own.
    pub fn graft(&self) -> Tree {
        let mut out = BTreeMap::new();
        for (path, items) in &self.branches {
            for (i, v) in items.iter().enumerate() {
                let mut p = path.clone();
                p.push(i as u32);
                out.insert(p, vec![v.clone()]);
            }
        }
        Tree { branches: out }
    }
}

/// How an input reshapes the data before the component sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modifier {
    #[default]
    None,
    Flatten,
    Graft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// A wire's start: output number `output` of node `from`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wire {
    pub from: NodeId,
    pub output: usize,
}

/// One input of a node: a typed-in value, or a wire that replaces it.
#[derive(Clone, Debug, PartialEq)]
pub struct Socket {
    pub constant: Value,
    pub wire: Option<Wire>,
    pub modifier: Modifier,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub component: &'static str,
    pub inputs: Vec<Socket>,
    /// Where the node sits on the canvas.
    pub at: (f32, f32),
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum GraphError {
    #[error("unknown component: {0}")]
    UnknownComponent(String),
    #[error("too many nodes (up to {MAX_NODES})")]
    TooManyNodes,
    #[error("no such node")]
    NoNode,
    #[error("no such input or output")]
    NoSocket,
    #[error("the wires form a loop")]
    Cycle,
    #[error("stopped")]
    Cancelled,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub nodes: BTreeMap<NodeId, Node>,
    next: u32,
}

impl Graph {
    pub fn new() -> Graph {
        Graph::default()
    }

    pub fn add_node(&mut self, component_name: &str, at: (f32, f32)) -> Result<NodeId, GraphError> {
        if self.nodes.len() >= MAX_NODES {
            return Err(GraphError::TooManyNodes);
        }
        let c = component(component_name).ok_or_else(|| GraphError::UnknownComponent(component_name.to_string()))?;
        let inputs = (c.defaults)().into_iter().map(|constant| Socket { constant, wire: None, modifier: Modifier::None }).collect();
        let id = NodeId(self.next);
        self.next = self.next.saturating_add(1);
        self.nodes.insert(id, Node { component: c.name, inputs, at });
        Ok(id)
    }

    /// Joins output `output` of `from` to input `input` of `to`, replacing any wire already there.
    pub fn connect(&mut self, from: NodeId, output: usize, to: NodeId, input: usize) -> Result<(), GraphError> {
        let outputs = self.nodes.get(&from).and_then(|n| component(n.component)).ok_or(GraphError::NoNode)?.outputs.len();
        if output >= outputs {
            return Err(GraphError::NoSocket);
        }
        let node = self.nodes.get_mut(&to).ok_or(GraphError::NoNode)?;
        let socket = node.inputs.get_mut(input).ok_or(GraphError::NoSocket)?;
        socket.wire = Some(Wire { from, output });
        Ok(())
    }

    pub fn disconnect(&mut self, to: NodeId, input: usize) -> Result<(), GraphError> {
        let node = self.nodes.get_mut(&to).ok_or(GraphError::NoNode)?;
        node.inputs.get_mut(input).ok_or(GraphError::NoSocket)?.wire = None;
        Ok(())
    }

    pub fn set_constant(&mut self, node: NodeId, input: usize, value: Value) -> Result<(), GraphError> {
        let node = self.nodes.get_mut(&node).ok_or(GraphError::NoNode)?;
        node.inputs.get_mut(input).ok_or(GraphError::NoSocket)?.constant = value;
        Ok(())
    }

    pub fn set_modifier(&mut self, node: NodeId, input: usize, modifier: Modifier) -> Result<(), GraphError> {
        let node = self.nodes.get_mut(&node).ok_or(GraphError::NoNode)?;
        node.inputs.get_mut(input).ok_or(GraphError::NoSocket)?.modifier = modifier;
        Ok(())
    }

    /// Removes a node and every wire that came from it.
    pub fn remove_node(&mut self, id: NodeId) {
        self.nodes.remove(&id);
        for n in self.nodes.values_mut() {
            for s in &mut n.inputs {
                if s.wire.is_some_and(|w| w.from == id) {
                    s.wire = None;
                }
            }
        }
    }

    /// Runs the graph. A node that fails is listed in `errors` and so are the nodes after it.
    pub fn evaluate(&self) -> Result<Evaluation, GraphError> {
        self.evaluate_until(&|| false)
    }

    /// Like [`Graph::evaluate`], but asks `stop` before each node and gives up with
    /// [`GraphError::Cancelled`] when it says yes, so a slow graph can be run off the window thread
    /// and be cancelled.
    pub fn evaluate_until(&self, stop: &dyn Fn() -> bool) -> Result<Evaluation, GraphError> {
        let mut indegree: HashMap<NodeId, usize> = HashMap::new();
        let mut after: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (id, node) in &self.nodes {
            let mut n = 0;
            for w in node.inputs.iter().filter_map(|s| s.wire) {
                if self.nodes.contains_key(&w.from) {
                    n += 1;
                    after.entry(w.from).or_default().push(*id);
                }
            }
            indegree.insert(*id, n);
        }
        let mut queue: VecDeque<NodeId> = indegree.iter().filter(|(_, n)| **n == 0).map(|(id, _)| *id).collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(id) = queue.pop_front() {
            order.push(id);
            for next in after.get(&id).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(n) = indegree.get_mut(next) {
                    *n = n.saturating_sub(1);
                    if *n == 0 {
                        queue.push_back(*next);
                    }
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err(GraphError::Cycle);
        }
        let mut ev = Evaluation::default();
        for id in order {
            if stop() {
                return Err(GraphError::Cancelled);
            }
            let Some(node) = self.nodes.get(&id) else { continue };
            let Some(c) = component(node.component) else { continue };
            match self.run_node(c, node, &ev) {
                Ok(trees) => {
                    for (k, t) in trees.into_iter().enumerate() {
                        ev.outputs.insert((id, k), t);
                    }
                }
                Err(e) => {
                    ev.errors.insert(id, e);
                }
            }
        }
        Ok(ev)
    }

    fn run_node(&self, c: &Component, node: &Node, ev: &Evaluation) -> Result<Vec<Tree>, String> {
        let mut trees: Vec<Tree> = Vec::with_capacity(node.inputs.len());
        for (i, s) in node.inputs.iter().enumerate() {
            let name = c.inputs.get(i).map(|x| x.name).unwrap_or("input");
            let t = match s.wire {
                Some(w) if self.nodes.contains_key(&w.from) => match ev.outputs.get(&(w.from, w.output)) {
                    Some(t) => t.clone(),
                    None => return Err(format!("'{name}' is fed by a node that has an error")),
                },
                _ => Tree::single(s.constant.clone()),
            };
            trees.push(match s.modifier {
                Modifier::None => t,
                Modifier::Flatten => t.flatten(),
                Modifier::Graft => t.graft(),
            });
        }
        match_and_run(c, &trees)
    }
}

/// What a run produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Evaluation {
    pub outputs: HashMap<(NodeId, usize), Tree>,
    pub errors: HashMap<NodeId, String>,
}

/// Runs a component over its inputs. The branches of all inputs are matched by path (an input
/// with a single branch serves every path), and inside a branch the lists are matched item by
/// item, the shorter ones repeating their last item.
fn match_and_run(c: &Component, trees: &[Tree]) -> Result<Vec<Tree>, String> {
    let mut paths: BTreeSet<&Vec<u32>> = BTreeSet::new();
    for t in trees {
        paths.extend(t.branches.keys());
    }
    let mut outs: Vec<Tree> = vec![Tree::default(); c.outputs.len()];
    let mut made = 0usize;
    for path in paths {
        let mut lists: Vec<Vec<Value>> = Vec::with_capacity(trees.len());
        for (i, t) in trees.iter().enumerate() {
            let branch = match t.branches.get(path) {
                Some(b) => Some(b),
                None if t.branches.len() == 1 => t.branches.values().next(),
                None => None,
            };
            let list = match branch {
                Some(b) if c.inputs.get(i).is_some_and(|x| x.gather) => vec![Value::List(b.clone())],
                Some(b) => b.clone(),
                None => Vec::new(),
            };
            lists.push(list);
        }
        if lists.iter().any(Vec::is_empty) {
            continue;
        }
        let n = lists.iter().map(Vec::len).max().unwrap_or(0);
        if n > MAX_ITEMS {
            return Err(format!("too many items (up to {MAX_ITEMS})"));
        }
        for j in 0..n {
            let args: Vec<Value> = lists.iter().filter_map(|l| l.get(j.min(l.len().saturating_sub(1))).cloned()).collect();
            if args.len() != lists.len() {
                return Err("input lists could not be matched".into());
            }
            let result = (c.run)(&args)?;
            if result.len() != outs.len() {
                return Err("the component made the wrong number of outputs".into());
            }
            for (out, items) in outs.iter_mut().zip(result) {
                made += items.len();
                if made > MAX_ITEMS {
                    return Err(format!("too many items (up to {MAX_ITEMS})"));
                }
                out.branches.entry(path.clone()).or_default().extend(items);
            }
        }
    }
    Ok(outs)
}

/// A generic Marlin-style printer set up from the numbers a node gives.
pub(crate) fn print_config(line_width: f64, nozzle: f64, bed: f64, walls: f64, spacing: f64) -> PrintConfig {
    PrintConfig {
        line_width,
        nozzle_temp: nozzle,
        bed_temp: bed,
        walls: walls.clamp(0.0, 8.0) as u32,
        infill_spacing: spacing.max(0.0),
        ..PrintConfig::default()
    }
}

#[cfg(test)]
mod tests;
