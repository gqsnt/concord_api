#!/usr/bin/env bash
# Installed by scripts/verify.sh as target/argv-guard/<tool> (python3, python, cargo, tar, rustfmt), ahead of
# the real tool on PATH. Windows limits a whole command line to 32,767 UTF-16 code units; Linux
# accepts far longer ones, so a script that passes bulk data as an argument fails only on Windows.
# This wrapper refuses such a command line on every platform, then runs the tool recorded in
# <tool>.real. The length is computed by argv-length.py (installed next to it) with the MSVC quoting
# rules; the arguments reach it on standard input, never as arguments.
dir=$(dirname "$0")
real=$(<"$0.real")
python=$(<"$dir/interpreter")
n=$(printf '%s\0' "$real" "$@" | "$python" -I "$dir/argv-length.py") || {
  echo "argv-guard: could not measure the command line for $(basename "$0")" >&2
  exit 126
}
n=${n%$'\r'}
if ((n > 32767)); then
  echo "argv-guard: a command line of $n UTF-16 units for $(basename "$0") exceeds the Windows limit of 32,767" >&2
  exit 126
fi
exec "$real" "$@"
