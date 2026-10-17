# YuraCAD Windows interactive acceptance test

Run this on a **real Windows desktop** (not the headless GitHub runner). Use the
exact binaries produced by one successful CI workflow. Do not label the release
"GUI verified" until the checklist has been performed on both programs.

## Preparing a candidate

Normal pushes and pull requests run checks and retain their build artifacts. They
do not replace the published download. In Actions, run **Build** on
`easy-layout-and-melt` with **prepare_draft** enabled only when a candidate is
wanted. After the Windows, Tests and Web jobs all succeed, the workflow creates a
new unpublished draft with a unique commit/run/attempt tag, both executables and
`SHA256SUMS.txt`. A branch-head check skips superseded runs.

Download both executables and their checksum file from that one draft, complete
the checks below, then obtain release authorization before publishing it. The
workflow never updates `latest`, overwrites an existing asset or publishes a
draft automatically. A Linux/software-GPU screenshot is not a Windows result.

## Provenance
- Branch / commit SHA: ______________________
- Actions workflow run URL: __________________
- Downloaded binary hashes match `SHA256SUMS.txt`: Yes / No
- Windows version / build: ___________________
- Display DPI / resolution: __________________
- GPU + driver: ______________________________
- Tester / date: _____________________________

## MELTCAD — `YURACAD.exe`
1. Launch by double-clicking. Confirm the main window and 3D viewport appear.
   Record whether the normal GPU path works and whether CPU fallback is used.
2. File > New. Verify the default scene is visible and can be edited.
3. Add a box. Save As `windows-smoke.meltcad` in an empty test directory.
4. Close/relaunch and open the saved project. Verify the box is present and
   has the same size and position; no unexpected unsaved-file prompt.
5. Export STL (suggested tolerance 0.5 mm) to `windows-smoke.stl`.
   Verify the file is nonempty and importable. Also export 3MF if available.
6. Quit and relaunch once to verify clean shutdown/restart.
7. Record actual steps, filenames, file sizes, screenshot references, crashes
   and `meltcad-log.txt` if any issue occurs.

## Classic — `YURACAD-Classic.exe`
1. Launch by double-clicking. Confirm **SolidCraft**, not MELTCAD, opens.
2. Create a new design, insert a box, save it as `windows-smoke.solidcraft`.
3. Close/relaunch, reopen the design, and verify the box is intact.
4. Export STL. Verify that the output is nonempty and can be imported.
5. Record any error, recovery prompt, or crash, including reproduction steps.

## Recording
- Native CI test result: PASS / FAIL / BLOCKED
- MELTCAD interactive launch: PASS / FAIL / NOT RUN
- MELTCAD new + save + reopen: PASS / FAIL / NOT RUN
- MELTCAD STL export + import: PASS / FAIL / NOT RUN
- Classic interactive launch: PASS / FAIL / NOT RUN
- Classic new + save + reopen: PASS / FAIL / NOT RUN
- Classic STL export + import: PASS / FAIL / NOT RUN

Do **not** consider a green Rust unit-test job, a PNG generated using
`--render`, or an existing Release asset to be evidence of these GUI steps.

## Action limits / billing
When GitHub reports that a job was not started due to billing, spending cap,
or minutes restrictions, record the exact Actions error text and run URL.
This is **BLOCKED**, not a passing build or an application code failure.
Do not merge PR #3 or publish a newly claimed tested release until runner
execution resumes and required checks pass.

