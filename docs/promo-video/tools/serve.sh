#!/bin/bash
# Start the virtual screen and the real Melt 3D window once; they stay up between takes.
pkill -x meltcad; pkill -x Xvfb; sleep 1
Xvfb :99 -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
sleep 2
cd /tmp
DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1 MELTCAD_BACKEND=gl nohup /home/claude/07egta/melt3d/target/release/meltcad --control 7788 >/tmp/app.out 2>&1 &
echo started
