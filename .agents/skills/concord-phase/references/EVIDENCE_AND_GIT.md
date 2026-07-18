# Concord evidence and Git procedure

## Baseline

Record:

```bash
git status --porcelain=v2 --branch
git rev-parse HEAD
git branch --show-current
git log --oneline --decorate -20
```

Verify recorded source/evidence/anchor commits exist and are ancestors. Stop for unexpected dirty,
staged, untracked, branch, worktree, or history state.

## Per-outcome patch loop

Before a source commit:

```bash
git diff --check
git diff --stat
git diff --no-ext-diff --binary \
  --output=git_diff/phase-<id>-unit-<unit>-uncommitted.patch
```

Read the complete patch. Correct blockers and rerun focused validation.

Stage exact paths only:

```bash
git add -- <path> [<path> ...]
git diff --cached --check
git diff --cached --stat
git diff --cached
```

Do not use `git add .`, `git add -A`, `git commit -a`, or commit amendment.

After the local commit:

```bash
git show --no-ext-diff --binary --format=fuller \
  --output=git_diff/phase-<id>-unit-<unit>.patch HEAD
git status --short
```

The tree must be clean before the next outcome.

## Final source identity

The final source commit includes all product source, tests, manifests, lockfile changes, generated
contracts, examples required for source proof, and executable validation configuration required to
test that source.

Run every applicable maintained and focused validation command against that exact immutable commit.
For each command record:

- timestamp;
- working directory;
- exact command;
- feature/environment configuration;
- tested source commit;
- exit status;
- concise result;
- unavailable or environment limitation.

Do not modify source while collecting final validation. A source correction creates a new source
commit and requires rerunning affected and cumulative checks.

## Evidence and documentation commit

After source validation, update only the approved documentation and conformance artifacts:

- current phase report;
- conformance evidence catalog;
- requirement coverage plan;
- implementation program;
- normative/architecture/decision documents only when the approved phase requires them;
- `docs/controller/PROJECT_CONTROL.toml`;
- README/examples documentation that is not part of executable source proof, when appropriate.

Historical accurate evidence rows retain their original requirement, path, command, and tested
commit. A moved test receives new coverage and new-phase evidence; old evidence is not rewritten.

Every new evidence row identifies:

- unique evidence ID;
- exact requirement ID;
- immutable tested source commit;
- exact test or command;
- feature configuration;
- direct or partial classification;
- precise limitation for partial evidence.

Do not claim unimplemented later-phase clauses.

## Anchor and self-reference

A commit cannot contain its own hash. Follow the repository's established source/evidence/anchor
convention:

1. final immutable source/test commit;
2. complete evidence/report/state commit;
3. documentation-only anchor only when needed to replace a documented pending self-reference or
   name the complete evidence state.

An anchor must not add product behavior, tests, manifests, lockfile changes, executable validation
configuration, evidence IDs, or changed classifications.

## Final integrity audit

Verify:

```bash
git status --porcelain=v2 --branch
git log --oneline --decorate -20
git diff --name-status <tested-source-commit>..HEAD
```

Classify every post-source file. If production source, tests, manifests, lockfiles, generated
contracts, or executable validation changed, rerun the applicable matrix against a new immutable
source commit.

Verify evidence counts and schema mechanically, exact requirement validity, unique evidence IDs,
referenced commit existence/ancestry, historical-row preservation, and `PROJECT_CONTROL.toml`
consistency.

Finish with a clean tree. `git_diff/` remains ignored and uncommitted.
