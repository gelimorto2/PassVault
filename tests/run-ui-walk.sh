#!/bin/bash
# Usage: run-ui-walk.sh <steps.js> <outdir> "<t1:name> <t2:name> ..."   (seconds after script start)
# Launches the debug build on the throwaway test vault (PASSVAULT_CONFIG_DIR) via XWayland and
# screenshots the window at the given times. Never touches the real vault or keyboard focus.
set -e
STEPS=$1; OUT=$2; SHOTS=$3; CFG=${PV_CFG:?set PV_CFG}
HERE=$(cd "$(dirname "$0")" && pwd)
CODE=$(python3 "$HERE/totp.py")
python3 - "$HERE/ui-walk.template.js" "$STEPS" "$CODE" > "$OUT/walk.js" <<'PY'
import sys
t=open(sys.argv[1]).read(); s=open(sys.argv[2]).read().replace("__CODE__", sys.argv[3])
print(t.replace("STEPS", s))
PY
cd "$HERE/../src-tauri"
GDK_BACKEND=x11 PASSVAULT_CONFIG_DIR=$CFG PASSVAULT_TEST_SCRIPT=$OUT/walk.js setsid ./target/debug/tauri-app > "$OUT/app.log" 2>&1 &
APP=$!
START=$(date +%s.%N)
sleep 2.5
WID=$(xdotool search --name '^PassVault$' | head -1)
for s in $SHOTS; do
  t=${s%%:*}; n=${s#*:}
  now=$(date +%s.%N); wait=$(echo "$START + 3 + $t - $now" | bc)
  [ "${wait:0:1}" != "-" ] && sleep "$wait"
  import -window "$WID" "$OUT/$n.png"
done
kill $APP 2>/dev/null || true
