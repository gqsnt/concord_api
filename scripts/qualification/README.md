# V3 qualification evidence

The V3 qualification workflow runs the maintained Linux implementation gate and
two independent measurement jobs. Each measurement job builds all comparison
inputs on its own runner, then measures them sequentially. Runtime and scale
results therefore have separate host identities.

The candidate defaults to the workflow commit. A manual run may select a full
candidate SHA. Source inputs, tools, commands, native exits, logs and generated
archives are retained as GitHub Actions artifacts. A missing terminal receipt is
an interrupted or incomplete command, never a pass. Artifact retention is
90 days; archive the evidence needed for longer-lived decisions.

The Python recorder rejects source mutation and preserves native failures.
Its tests include a nonzero process, changed tracked input, killed timed-out
process and truncated/duplicate probe output. It is also usable locally:

    python3 -m unittest discover -s scripts/qualification -p 'test_*.py'
    python3 scripts/qualification/evidence.py record \
      --source /path/to/clean/concord --out /path/to/evidence \
      --label implementation -- just verify-implementation

Qualification omits debug symbols to fit bounded runner storage. It retains
debug assertions and the unoptimized test profile. Performance resets these
overrides: release runtime probes and ordinary debug scale fixtures are compared
with two Cargo build jobs and incremental compilation disabled.

The historical checkout is only an isolated measurement reference. The current
product and its tools continue to accept V3 alone. Runtime probes use the same
operations and semantic declarations; external package versions and checksums
must match before measuring. The feature graphs and actual input patches are
retained separately.

Runtime collects five fresh processes per probe/category for every candidate,
including nine individual first-use categories and their ordered sequence.
Medians, minima and maxima describe those process observations. The warm probe
already computes its own batch median; the outer median is a median of batch
medians. It does not include server-thread allocations, RSS, external network
latency or remote OAuth-provider behavior.

Scale covers 1, 10, 100, 500 and 1000 endpoints, plus two additional paired runs
at 1000. The same final scale harness measures both V3 inputs. The historical
harness exists only in its detached historical checkout. Symmetric source and
expansion captures occur outside measured intervals; their patches are recorded.
Cold means the fixture package was cleaned with dependencies still built.
Serialized description bytes are not retained heap size. Debug executable size
includes debug information and description code.

Successful execution establishes complete observations. It does not automatically
accept the performance obligation, human comprehension, a compatibility freeze,
integration into main, or package publication. Material costs still require
source-based explanation or a current discriminating measurement.
