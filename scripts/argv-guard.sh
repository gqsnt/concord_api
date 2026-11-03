#!/usr/bin/env bash
# Installed by scripts/verify.sh as target/argv-guard/<tool> (python3, python, cargo, tar), ahead of
# the real tool on PATH. Windows limits a whole command line to 32,767 characters; Linux accepts far
# longer ones, so a script that passes bulk data as an argument fails only on Windows. This wrapper
# refuses such a command line on every platform, then runs the tool recorded in <tool>.real. The
# count is a lower bound of the Windows length: Windows quoting only adds to it.
real=$(<"$0.real")
n=${#real}
for arg in "$@"; do
  quotes=${arg//[^\\\"]/}
  n=$((n + ${#arg} + ${#quotes} + 3))
done
if ((n > 32767)); then
  echo "argv-guard: a $n-character command line for $(basename "$0") exceeds the Windows limit of 32,767" >&2
  exit 126
fi
exec "$real" "$@"
