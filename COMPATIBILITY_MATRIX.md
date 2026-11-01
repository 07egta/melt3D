# Compatibility is feature-specific

| Capability | Current evidence / status |
|---|---|
| Independent Arclet desktop shell/settings | Existing Linux app executed; no remote Melt3D changes |
| NURBS curve evaluation / G0-G3 curve-end analysis | Baseline implementation retained; numerical hardening patch and 7 regression tests added, uncompiled |
| Curve Rebuild | Existing GUI accept/undo/redo/save/reload tested on one sample; deviation is sampled, not a certified maximum |
| Native `.sc` design format | Curve and embedded node mesh cold round-trips verified in original binary |
| Node JSON format | Version-1 original format retained; normal/invalid/resource-limit GUI tests pass |
| 2,000-node evaluation | Simple Number chain verified; no claim for 2,000 complex surfaces |
| Unsaved graph Close protection | Defect reproduced; patch + 11 state/UI tests added, uncompiled |
| Normal-exit graph preferences recovery | Edited value 31 restored as unsaved in original binary; crash/power-loss not tested |
| STL export | Node mesh 192 triangles exactly preserved across native save/reload; torus meshes finite and closed by edge incidence |
| STEP | One analytic torus exported/imported/re-exported; mm units retained; representation changes; partial smoke coverage only |
| IGES / OBJ / GLB / glTF | Not exercised in this session; no new compatibility claim |
| 3DM | No new implementation or Rhino round-trip test; not claimed compatible |
| GH / GHX | Unsupported in this delivery |
| RhinoCommon / Python / .rhp / .gha binary plugins | Unsupported in this delivery |
| G3 surface continuity | Not implemented/verified as a general capability |
| Class-A certification / xNURBS / PolySplines equivalence | Not claimed; names and visible smoothness do not establish equivalence |
| Linux original executable | GUI workflow tests on Xvfb + software renderer |
| Linux new Rust patches | Not built |
| Windows / macOS | No new executable; no native launch test |
| WebAssembly / browser | Historical baseline code-check records retained; not rerun; actual browser unverified |
| Intel GPU / performance comparison with Rhino or Plasticity | Not measured |
| Bambu A1 / A1 mini G-code / AMS | Not tested; do not print unverified generic output |

File compatibility, API compatibility and native plugin execution compatibility are not interchangeable.
