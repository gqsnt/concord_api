"""Execute locally and offline; record commands, exit status, and unabridged output."""
from pathlib import Path
import json
import os
import subprocess
import time

ROOT = Path.cwd()
BASE = ROOT / 'target/probes'
results = []

def run(label, args, *, target=None, expected=0):
    env = dict(os.environ, CARGO_NET_OFFLINE='true')
    if target:
        env['CARGO_TARGET_DIR'] = str(target)
    log = BASE / f'{label}.txt'
    start = time.monotonic()
    print(f'START {label}: {args}', flush=True)
    with log.open('w') as out:
        proc = subprocess.run(args, env=env, stdout=out, stderr=subprocess.STDOUT, timeout=900)
    row = {'label': label, 'command': args, 'exit_code': proc.returncode,
           'expected_exit': expected, 'elapsed_seconds': round(time.monotonic()-start, 3),
           'target': env.get('CARGO_TARGET_DIR'), 'log': str(log.relative_to(ROOT))}
    results.append(row)
    (BASE / 'execution-results.json').write_text(json.dumps(results, indent=2)+'\n')
    print(f'END {label}: exit={proc.returncode}, expected={expected}, seconds={row["elapsed_seconds"]}', flush=True)
    if proc.returncode != expected:
        print(log.read_text()[-6000:], flush=True)
    return row

for package in ['migration', 'matrix', 'renamed', 'hints', 'raw-keyword']:
    run(f'lock-{package}', ['cargo', '+1.85', 'generate-lockfile', '--offline', '--manifest-path', str(BASE/package/'Cargo.toml')])

# Exactly one invocation of the requested repository tests, using its pinned toolchain.
run('formats-ui-pinned', ['cargo', 'test', '--locked', '--offline', '-p', 'concord', '--test', 'formats', '--test', 'ui'])

for label, tc, target in [('pinned', '1.97.1', ROOT/'target/review-build'),
                           ('stable', 'stable', BASE/'build-stable'),
                           ('msrv', '1.85', BASE/'build-msrv')]:
    for package in ['migration', 'matrix', 'renamed']:
        run(f'{package}-{label}', ['cargo', f'+{tc}', 'test', '--locked', '--offline', '--manifest-path', str(BASE/package/'Cargo.toml'), '--lib', '--', '--nocapture'], target=target)
    run(f'absolute-{label}', ['cargo', f'+{tc}', 'check', '--locked', '--offline', '--manifest-path', str(BASE/'renamed/Cargo.toml'), '--bins'], target=target)
    run(f'hints-{label}', ['cargo', f'+{tc}', 'check', '--locked', '--offline', '--manifest-path', str(BASE/'hints/Cargo.toml'), '--lib', '--message-format=json'], target=target, expected=101)
    for binary in ['keyword_leftover', 'keyword_suggested', 'keyword_corrected', 'raw_json_leftover', 'raw_json_suggested', 'raw_json_corrected']:
        expected = 101 if binary.endswith('leftover') or binary == 'keyword_suggested' else 0
        run(f'{binary}-{label}', ['cargo', f'+{tc}', 'check', '--locked', '--offline', '--manifest-path', str(BASE/'raw-keyword/Cargo.toml'), '--bin', binary], target=target, expected=expected)

print('Execution complete.', flush=True)
