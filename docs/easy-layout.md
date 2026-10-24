# EASY layout

The EASY layout is a simpler way to work. It puts the commands most people use in one place. Nothing is removed: every command is still on the other tabs, in the command search, and in the command line.

## The EASY tab

The EASY tab has seven panels, left to right, in the order you usually work:

- **DRAW**: sketch tools such as line, rectangles, circles, arcs, polygon, spline, ellipse, text, offset, trim, fillet, mirror, and SVG/DXF import.
- **SHAPE**: extrude, revolve, sweep, loft, pipe, box, cylinder, sphere, torus, coil, hole, rib, emboss.
- **SURFACE**: surface versions of the same ideas, plus patch, blend, stitch, trim, thicken.
- **REFINE**: fillet, chamfer, combine (booleans), shell, melt, press-pull, draft, split, replace face.
- **ARRANGE**: move, mirror, scale, rectangular, circular and path patterns, align.
- **CHECK**: measure, zebra, curvature, environment map, isocurve, minimum radius, section, draft analysis.
- **OUTPUT**: save, save as mesh (STL, 3MF), export (STEP and others), insert STEP or mesh, parameters.

The first few commands in each panel get large buttons. Commands that are planned but not built yet are left out.

## Search: F or S

Press **F** (or **S**) to open the command search. It lists every command. Matching ignores case. It also knows the other names a command goes by: other common English names and Japanese names. Examples from the alias table in `easy.rs`:

| You type | Command |
|---|---|
| `Sweep1` | Sweep (`solid.sweep`) |
| `FilletEdge` | Fillet (`solid.fillet`) |
| `BooleanUnion` | Combine (`solid.combine`) |
| `ArrayPolar` | Circular pattern (`solid.pattern.circular`) |
| `押し出し` | Extrude (`solid.extrude`) |

## Drawing starts a sketch for you

The drawing tools (line, rectangles, circles, arcs, polygon, splines, ellipse, point) open a sketch on the XY plane when no sketch is open. You do not have to pick a plane first. The plane buttons in the settings panel do the same for XY, YZ or XZ.

## The screen around the viewport

- **Tool strip (left):** the modelling tools in one column. While a sketch is open, it shows sketch tools instead, ending with Finish Sketch. Tools that do not fit the window height sit behind the **…** button, which opens the search. A dimmed tool is not available right now.
- **Selection chips (top left):** body, face, edge, vertex. A lit chip means a click can pick that kind of thing. Click a chip to turn it on or off. Two small buttons at the right of the chips show or hide the settings panel and the ribbon.
- **Settings panel (right):**
  - *Plane:* XY, YZ and XZ start a sketch on that plane. "Make plane" offers construction planes (offset, three points, midplane, at angle, tangent, perpendicular, along a path, two edges).
  - *Display:* shaded with edges, shaded, wireframe, hidden lines; switches for grid, origin, ground shadow and sketches.
  - *Camera:* a perspective switch, and Reset Camera, which returns to the home view.

## Melt (`solid.melt`)

Melt blends closed solids together, like clay or liquid flowing into itself. It has two modes:

- **join** (default): melts the bodies into one soft shape.
- **cut**: removes the later bodies from the first one, leaving a soft edge.

Parameters (from the command's params list in `melt.rs`):

- `bodies`: names of the bodies, two or more. Required.
- `strength`: blend reach in mm, default 5, range 0 to 1000. 0 gives a sharp join or cut.
- `mode`: `join` or `cut`. Default `join`.
- `resolution`: cells along the longest side of the shapes, 16 to 256, default 96. Higher is smoother and slower. The grid also reaches `strength` past the shapes on every side, so a big strength uses more cells; if the grid gets too big, the command says so and does nothing.
- `smooth`: smoothing rounds, 0 to 8, default 2.
- `keep`: keep the original bodies. Default false, so they are removed.
- `name`: name of the new body. Default `Melt`.

The result is one new body in the timeline. The reply gives its volume, triangle count and the strength used.

Example:

```json
{"command":"solid.melt","params":{"bodies":["A","B"],"strength":8,"resolution":96}}
```

**Melt output is a mesh, not an exact B-rep.** The shape is rebuilt from a grid of triangles, so its faces are not exact curved faces. Use it for 3D printing and organic shapes. Do not use it when you need to edit exact faces or edges later (for example, filleting one particular edge). Build those features first, then melt if you still need the blend.
