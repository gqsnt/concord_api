#!/usr/bin/env bash
# Runs one Phase B role session with Codex and keeps its log (the session header records the
# observed model, reasoning effort and sandbox). Usage:
#   run_codex.sh MODEL WORKTREE TARGET_DIR PROMPT_FILE LOG_PREFIX
# Codex workspace-write cannot write .git: the coordinator commits the session's files.
set -uo pipefail
model=$1 worktree=$2 target=$3 prompt=$4 log=$5
mkdir -p "$target"
codex exec -m "$model" -c model_reasoning_effort=xhigh -s workspace-write \
  -c sandbox_workspace_write.network_access=true --add-dir "$target" \
  -C "$worktree" -o "$log.last.md" - <"$prompt" >"$log.log" 2>&1
echo "exit=$?" >>"$log.log"
