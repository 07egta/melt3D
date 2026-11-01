# Verification record - 2026-11-01 continuation

## Critical scope distinction

**New Rust changes are source-only and have not been compiled.** There is no Rust toolchain or complete base checkout in this execution environment. All GUI observations below use the original recovered `f97df2e` Linux executable, SHA-256 `2f0d5d3bbf88b08c2c7845d959e23e6c2192c0b97e9c78564cd4e54a159c1a0a`.

Tests ran in Xvfb with Mesa software rendering, not an Intel GPU or a Windows desktop. Actual GUI buttons, parsers, file workers, evaluation and geometry export were exercised. Because a system file chooser was unavailable, selection was supplied by a **private scripted XDG portal fixture**. This does not verify native chooser usability.

## Source and mathematical checks actually executed

| Check | Result | Evidence |
|---|---|---|
| Recover complete new files from old patches | 47/47 Git blob hashes match | `qa/scripts/recovery-verification.json` |
| Apply both new patches to recovered baseline files | `git apply --check` and resulting SHA-256 hashes pass | `source-changes.json` |
| Independent numerical oracle | 7,492 assertions; zero failures | `qa/numerics/verification.json` |
| Delivery safety tests | 8 Python unittest cases pass | `qa/logs/delivery-tools-tests.log` |
| New Rust regression tests | 18 added, **not executed** | source diff and `source-changes.json` |
| Rust compilation / cargo test / clippy / fmt | **Not run** | Environment lacks Rust toolchain |
| Independent AI/subagent review | **Not available; none claimed** | No usable subagent tool |

The numerical verifier reproduces the proposed scalar arithmetic and compares it with SciPy's compiled B-spline basis and 90-digit mpmath rational Bernstein derivatives. It covers degrees 1-7, nonuniform/repeated knots, clamped endpoints, common weight scales, translated curves, finite extreme coordinates, stationary spans, tiny parameter domains and reparameterized arc-length jets. These are **not 7,492 Rust unit tests**, and not a guarantee of all possible G3/Class-A cases.

For the translated unit quarter-circle at coordinates of order 1e12, the sampled old arithmetic had a maximum third-derivative absolute error of `9.366140265841816e-4`; the new scalar reproduction gave `2.220446049250313e-15` on that fixture. Position accuracy remains limited by the precision of input coordinates. Overflow and unrepresentable cases still require explicit errors, not success claims.

## GUI workflows actually executed on the old binary

1. **Curve editing and native project round-trip.** Open the existing Rebuild sample, accept the rebuilt curve, finish the sketch, save, undo, save, redo and save. The two-curve state returns exactly after redo. Reopen in a separate process and save under another name: feature data and millimetre units match. The project name changes with Save As, as expected.
2. **Node-to-model workflow.** Change Number to 27, save graph JSON, load it in a separate process, evaluate Circle -> Extrude, use Add to model, save a `.sc` project, export STL, reopen the `.sc` in another process and re-export. Both exports have 192 triangles and bounds `[-10,-10,0]` to `[10,10,27]`. All binary triangle records are identical. Only the STL filename header differs.
3. **Unsaved-node quit defect.** After saving Number=27, change it to 31 and deliver an X11 WM_DELETE_WINDOW event. The old program exits without a graph save/discard prompt while the graph file remains 27. The preferences snapshot contains 31 with `dirty=true`, and reopening a copied snapshot shows 31 as unsaved. Therefore the defect is missing explicit save/discard control, **not proven irreversible loss in this normal-exit case**. The new guard is source-only pending Rust/GUI validation.
4. **Malformed graph isolation.** Cyclic connection, unknown component, duplicate ID and truncated JSON are rejected. Saving after each rejected load yields exactly the previously loaded valid graph. Screenshots and saved files are included.
5. **Resource boundary.** A 2,000-node chain of Number nodes loads and evaluates; the last node displays 1234. A 2,001-node file is rejected, and saving afterwards preserves the 2,000-node graph exactly. This is not a benchmark of 2,000 complicated CAD operators.
6. **STEP smoke round-trip.** A torus with major radius 28 mm and minor radius 8 mm exports to AP242 STEP, imports in a separate process, saves as a project and re-exports. The input has TOROIDAL_SURFACE entities; re-export uses SURFACE_OF_REVOLUTION entities and an assembly wrapper. **The STEP representation is not identical.** Millimetre units, one closed-shell solid and two faces are retained in this example. General STEP compatibility and third-party application round-trips are not certified.

## STEP geometric cross-check

The original torus STL contains 47,676 triangles; the imported torus export contains 51,912. Every indexed coordinate is finite; every undirected geometric edge has incidence two. For exported float32 vertices, distance to the analytic torus `(sqrt(x*x+y*y)-28)^2 + z*z = 64` is at most:

- Original: `1.8696636470139083e-6 mm`.
- After STEP import: `1.9180577446320513e-6 mm`.

This checks vertex locations, **not the maximum deviation across triangle interiors or a certified B-rep Hausdorff bound**. Mesh volumes are approximately 35,330.46 and 35,337.28 cubic mm versus analytic 35,372.66 cubic mm; tessellation changes explain why the two meshes are not byte-identical. See `qa/workflows/step-roundtrip-result.json`.

## Baseline process measurements

Five fresh-HOME runs of `arclet --sample --screenshot ...` exited with code 0. Median process lifetime: **1.5847 s**; median sampled peak RSS: **305.34 MiB**. This includes sample creation, rendering at least 12 frames, PNG capture and shutdown. It is **not first-frame startup latency**, not a hardware-GPU FPS measurement, and not a before/after speedup. RSS is sampled around every 10 ms. Mesa shader caches are fresh per run; OS page caches are not flushed.

## Test harness corrections

Early clicks missed keyboard focus or moved toolbar coordinates; those attempts are not counted as successes. XSync was needed before closing the synthetic X11 event sender. One temporary export target was reused accidentally during QA; the node cold-reload STL was regenerated and byte-record equality reverified. Selected screenshots exclude these ambiguous attempts. Application observations and fixture failures are kept separate.

## Still not verified

New Rust source compilation; new close dialog interaction; new numerical code executed as Rust; Windows/macOS/browser; actual Intel GPU; crash/power-loss recovery; broad B-rep Boolean/fillet reliability; 3DM in either direction; GH/GHX; RhinoCommon/.rhp/.gha; Class-A certification; surface G3; xNURBS-equivalent generation; Bambu printer output. Historical baseline logs in `baseline-delivery/` are retained but not rerun or added to this session's pass counts.
