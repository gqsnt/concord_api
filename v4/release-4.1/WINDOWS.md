# Windows failures and their root causes

## Windows run 41635670821, job 125916104134 (head 31984ae)

Raw logs: the work directory's v41/ci/run-41635670821/ (SHA256SUMS). Gate summary: 30 of 31 internal steps PASS; one FAIL.

| Step | Result | Class | Exact error |
|---|---|---|---|
| consumers | FAIL | portable tooling (gate script) | `scripts/consumers.sh: line 53: /c/hostedtoolcache/windows/Python/3.12.10/x64/python3: Argument list too long` |

Every other step passed (tests: 650 workspace, all feature subsets). The Windows job ran
`scripts/verify.sh` without `--package --bench`, so package-check, consumers-packaged and
bench-build never ran on Windows (the packaged consumers run the same faulty line).

Root cause: consumers.sh passed the whole `cargo metadata --no-deps` JSON (51,763 bytes on Linux)
to Python as argv[3]. Windows CreateProcess limits the whole command line to 32,767 characters;
Linux allows 131,072 per argument, so Linux passed. Not a library defect; not environmental.

Regression evidence (Linux, argv guard installed): the original consumers.sh fails with
`argv-guard: a 57668-character command line for python3 exceeds the Windows limit of 32,767`.

## Windows run 41644184361, job 125945046735 (head 15b3200)

The consumers fix holds natively (`consumers` PASS, gzip consumer included). Running the full
inventory on Windows for the first time exposed two more failures, same root cause:

| Step | Result | Class | Exact error |
|---|---|---|---|
| package-check | FAIL | portable tooling | `tar (child): Cannot connect to D: resolve failed` / `gzip: stdin: unexpected end of file` / `/usr/bin/tar: Child returned status 128` |
| consumers-packaged | FAIL | portable tooling | the same three lines |

Root cause: both scripts ran `tar -xzf "$target_dir/package/X.crate"` with `$target_dir` from
`cargo metadata` (`D:\a\concord_api\concord_api\target`); GNU tar treats a name with a colon before
its first slash as a remote `host:path`. Reproduced on Linux with GNU tar 1.35 and a file named
`D:\concord-core-4.1.0.crate` (identical message); `tar -xzf - -C out < 'D:\...'` succeeds.
Fix: archives on standard input; `check-ci.py --workflow` fails on any `tar -f NAME` in scripts.
Linux job 125945047304 and nightly 125945047260 passed (35 steps, 651 tests).

## Windows run 41646913735, job 125954551029 (head 791e991)

`consumers-packaged` now PASSES natively. One failure left:

| Step | Result | Class | Exact error |
|---|---|---|---|
| package-check | FAIL | portable tooling | `scripts/package-check.sh: line 33: D:\a\concord_api\concord_api\target` (line break) `/package/concord-core-4.1.0.crate: No such file or directory` |

Root cause: `read -r version target_dir < <(... | python3 -c 'print(...)')`; Python on Windows writes
CR LF, `read` keeps the CR, so the path ends in `\r` (the line break in the message). Git Bash's
`$(...)` drops it, which is why consumers.sh (command substitution) was unaffected. It was masked
until the tar fix because tar failed first. Reproduced in bash: `printf 'ok %s\r\n' 'D:\x' | read -r v t`
gives `$'D:\\x\r'`. Fix: strip a trailing CR after the read (the only `read` from a process in scripts/).
Linux job 125954551098 and nightly passed.
