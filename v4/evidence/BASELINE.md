# Gate 0 — repository preservation and baseline

Recorded: 2026-11-01T17:06+01:00 by the V4 coordinator (Claude Opus 5.5, `claude-opus-5-5`, effort xhigh, Claude Code 2.1.314, desktop Code tab).

## Repository state found at start

| Item | Value |
|---|---|
| Main checkout | `/home/canarit/projects/concord_api` |
| Checked-out branch | `concord-v3` at `570eec460dee1d83828ae48e35396ae07652aabc` |
| `origin/concord-v3` | `5209ab5c5ec8669be26224d154142ca4cf9ed64e` (local `concord-v3` is 3 commits behind; not pulled) |
| `origin/main` | `a492d3678e8330307cd325a05ca0ee72499e9726` (merge of PR #1 = `concord-v3` @ `5209ab5`; identical tree) |
| Local `main` | `3765907` (stale, V2.1-era; untouched) |
| Tracked modifications | none |
| Untracked | `concord_v4_handoff/` (supplied handoff bundle), `.claude/agents/concord-v4-*.md` (six installed subagent profiles, identical to `concord_v4_handoff/agents/`) — both are supplied instructions, not user edits and not V4 implementation |
| Stashes / other worktrees | none |
| V3 size | 1,037 tracked files, ~253k lines; workspace `3.0.0-beta.1`, 7 crates + xtask/perf + 2 downstream crates |

## Preservation actions (all non-destructive, local only)

1. Annotated local tag `concord-v3-preserved` → `a492d36` (final V3 state including the 3 commits the local branch had not pulled).
2. New branch `concord-v4` created from `a492d36` in an isolated worktree: `/home/canarit/projects/concord_api_v4`.
3. Detached read-only reference worktree of V3 at `a492d36`: `/home/canarit/projects/concord_api_v3ref` (used by research agents, Codex and baseline builds; never edited).
4. Handoff bundle and agent profiles copied (untracked) into the V4 worktree so the reviewer script works from that root. They are not committed.
5. The user's main checkout (`concord-v3` @ `570eec4`) was not modified, switched, pulled or cleaned. Nothing was pushed; remotes are unchanged.

V3 is recoverable from: tag `concord-v3-preserved`, `origin/main`, `origin/concord-v3`, local `concord-v3`.

## Toolchain and machine

| Item | Value |
|---|---|
| rustc / cargo | 1.97.1 (8bab26f4f 2026-07-14) / 1.97.1 |
| rustfmt / clippy | 1.9.0-stable / 0.1.97 |
| `rust-toolchain.toml` (V3) | channel 1.97.1; workspace `rust-version = "1.86"` |
| OS | Linux 7.0.0-42-generic x86_64 (Ubuntu) |
| CPU / RAM | Intel i5-13600K (20 threads) / 30 GiB |
| Codex CLI | codex-cli 0.166.0, `~/.codex/config.toml`: `model = "gpt-6.1-sol"`, `model_reasoning_effort = "xhigh"`; global `~/.codex/AGENTS.md` is empty |
| Windows / macOS | not available in this environment — any Windows/macOS claim is `NOT_RUN` |
| Network | cargo registry cache contains reqwest 0.13.4/0.13.5, tokio 1.53, serde_json, url 2.5.8, http 1.4, syn 3.0.x, trybuild; builds use `--offline` |

## V3 baseline build

See `v3_baseline.md` (filled when the background build finishes). It is measured only to compare compile cost; V3 test results do not certify V4.
