#!/bin/bash
# run.sh <makepad/relax binary> w64 [workdir]: web_diff.rs under Relax's web target (wasm64) against rustc's
# host build of it (wasm64's data layout is x86_64's). Prints "web_diff w64: N lines equal" or the diff.
# (Its wasm32 mode and the wasm export's run needed node for their reference: no check runs node.)
set -u
H=$(cd "$(dirname "$0")" && pwd)
B=$1
W=$2
T=${3:-$(mktemp -d)}
mkdir -p "$T/u"
case "$W" in
  w64) TRIPLE=wasm64-unknown-unknown
       if [ -n "${REF:-}" ]; then cp "$REF" "$T/ref.txt"; else
       rustc --edition 2021 -O "$H/web_diff.rs" -o "$T/ref" 2>"$T/rustc.err" || { cat "$T/rustc.err"; exit 2; }
       "$T/ref" 2> "$T/ref.txt" || exit 2; fi ;;
  *) echo "usage: run.sh <binary> w64 [workdir]"; exit 2 ;;
esac
cat > "$T/u/units.json" <<J
{"version":1,"roots":[0],"units":[{"pkg_id":"path+file://$T#web_diff@0.0.0","target":{"kind":["bin"],"name":"web_diff","src_path":"$H/web_diff.rs","edition":"2021"},"mode":"build","features":[],"dependencies":[]}]}
J
RELAX_CHECK_TARGET=$TRIPLE RELAX_STD=$H/../../std RELAX_CACHE_DIR=$T/tgt "$B" run --quiet --no-live --units "$T/u/units.json" web_diff --release > "$T/relax.out" 2> "$T/relax.err"
# (the program's own lines: not the run's own (makepad's, relax's) nor its frames' paths)
grep -v "^makepad\|^relax: \|^  /" "$T/relax.err" > "$T/relax.txt"
if diff "$T/ref.txt" "$T/relax.txt" > "$T/diff"; then
  echo "web_diff $W: $(wc -l < "$T/ref.txt" | tr -d ' ') lines equal"
else
  head -40 "$T/diff"
  grep "^makepad\|^relax: " "$T/relax.err" | cut -c1-300 | head -8
  echo "web_diff $W: DIFFERENT (log $T/relax.err)"
  exit 1
fi
