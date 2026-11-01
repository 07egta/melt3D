# Decisions and trade-offs

## Keep the recovered implementation
Do not restart from a UI mockup or overwrite Melt3D. Reuse the already developed independent Arclet shell and reusable CAD engine components. Restore into a new local project with pushes to the source repository disabled. A separate public GitHub repository and Windows release are not created in this environment.

## Stabilize rational jets before adding surface promises
The quotient-rule derivatives of homogeneous world coordinates are mathematically valid but ill-conditioned under large translations. Evaluate relative to an active control point, compensate short sums, divide by the actual normalized denominator, and restore the origin last. Dominant-contribution anchoring protects tiny clamped endpoints. Half-scaling handles finite opposite extreme coordinates. Constant local spans return exact zero derivatives before large derivative bases are formed.

Alternatives considered: leave global sums and merely increase tolerances (hides errors); globally rescale all curves (can erase small endpoints); introduce multiprecision into the runtime (higher cost/dependencies without solving interaction requirements). The chosen fix preserves the existing API and dependency set. Runtime speedup is not claimed; native performance needs measuring after compilation.

## Treat graphs as real editable documents at Close
The shared document quitter knows CAD tabs but not the independent graph. Guard both OS and internal quit paths. Save completion, not button click, must clear dirty status. An explicit discard applies only to the exact graph snapshot. Store an immutable/copy-on-write graph snapshot rather than serializing megabytes on every frame. Busy file work and pending mesh insertion block exit. A graph discard never automatically discards a dirty CAD design.

Mark only the untouched built-in demo clean. Restored dirty work explicitly clears its saved snapshot even when its content equals the demo. Unknown/failed saves remain unsaved.

## Do not add an untested foreign kernel or compatibility facade
No new OpenCASCADE, openNURBS, proprietary library or runtime bridge is integrated in this patch. Kernel selection for a future independent adapter requires its own licensing review and measured geometry tests. A .3dm extension or similarly named command is not evidence of Rhino compatibility.

## Package evidence honestly
A hidden stale executable in the old archive can be removed without altering the working binary. Repacking reduces delivery size, not CPU or RAM. The repack is clearly labelled baseline. New source changes and their verification gaps are delivered separately. All original licenses and notices are retained; no new runtime dependency is added.
