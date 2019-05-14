#!/bin/sh
set -ex

ls -lF -R target/debug/junction-test-*

GDB_STARTUP_FILE=debugs.gdb

cat > "${GDB_STARTUP_FILE}" << EOF
set disassembly-flavor intel
set auto-load off
set confirm off
set pagination off
set verbose off
dir src/
r --test-threads=1 --nocapture
bt
continue
q
EOF

cat "${GDB_STARTUP_FILE}"

for EXE in target/"${TARGET}"/debug/deps/junction-*.exe; do
  if [ -x "${EXE}" ]; then
    gdb \
      --batch \
      -q \
      -x "${GDB_STARTUP_FILE}" \
      "${EXE}";
  fi
done
