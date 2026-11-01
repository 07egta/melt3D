#!/usr/bin/env python3
"""Restore Arclet from published patches into a fresh checkout.

A NEW directory is required; existing application worktrees are never modified.
The resulting tree is uncompiled and unverified until local tests are run.
"""
from pathlib import Path
import argparse, subprocess, hashlib, json, sys
BASE = "a309790ac8e265b918da09474a02b069609ab3b5"
UPSTREAM = "https://github.com/07egta/melt3D.git"
ROOT = Path(__file__).resolve().parent.parent

def run(*args):
    subprocess.run(args, check=True)

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("destination", type=Path)
    p.add_argument("--existing-base", type=Path, help="a separate local Git checkout containing the pinned base commit")
    a = p.parse_args()
    destination = a.destination.expanduser().resolve()
    if destination.exists():
        p.error("Destination must not exist; no existing worktree will be modified")
    manifest = json.loads((ROOT / "SHA256_MANIFEST.json").read_text(encoding="utf-8"))
    for rel, expected in manifest.items():
        path = (ROOT / rel).resolve()
        if not path.is_relative_to(ROOT.resolve()) or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise RuntimeError(f"Patch integrity failed: {rel}")
    if a.existing_base:
        run("git", "clone", "--no-hardlinks", "--no-checkout", str(a.existing_base.expanduser().resolve()), str(destination))
    else:
        run("git", "clone", "--no-checkout", UPSTREAM, str(destination))
    run("git", "-C", str(destination), "checkout", "--detach", BASE)
    for patch in sorted((ROOT / "patches").glob("*.patch")):
        run("git", "-C", str(destination), "apply", "--check", str(patch))
        run("git", "-C", str(destination), "apply", str(patch))
    run("git", "-C", str(destination), "switch", "-c", "arclet/standalone-local")
    print("Restored source; changes are uncommitted. Build and test before releasing.")
    print("Melt3D source repository was not modified.")
if __name__ == "__main__":
    try: main()
    except (OSError, subprocess.CalledProcessError, RuntimeError) as e:
        print(f"Restore stopped: {e}",file=sys.stderr); sys.exit(1)
