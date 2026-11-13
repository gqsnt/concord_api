#!/usr/bin/env python3
"""Prepare, or run once via build/limited.sh, the 100/500 endpoint and 200-input measurements.

Run: build/limited.sh python3 api/real-gen/tools/measure.py --run
All commands run sequentially with four Cargo jobs. Dependencies may be warm;
the generated client packages have distinct names and are measured check, build.
"""
from pathlib import Path
import argparse
import json
import os
import re
import subprocess
import time

API = Path(__file__).resolve().parents[2]
OUTPUT = API / 'target' / 'p44-measure'


def prepare(count, wide=False):
    label = "wide200" if wide else f"n{count}"
    root = OUTPUT / label
    (root / 'src').mkdir(parents=True, exist_ok=True)
    (root / 'Cargo.toml').write_text(f'''[package]
name = "mixed-{label}"
version = "0.0.0"
edition = "2021"
publish = false
[workspace]
[dependencies]
concord = {{ path = "{API / 'concord'}" }}
serde = {{ version = "1", features = ["derive"] }}
''')
    endpoints = []
    for i in range(count):
        endpoints.append([
            f'GET get_{i}(id: u64, filter?: String) -> Item {{ path "items/{{id}}"; query {{ filter; }} }}',
            f'POST create_{i}(item: Item) -> Item {{ path "items"; body json(item); idempotent; }}',
            f'GET page_{i}(start?: u32, count?: u32) -> Vec<Item> {{ path "items"; query {{ start; count; }} paginate by_offset_limit(start, count); }}',
            f'HEAD exists_{i}(id: u64) -> bool {{ path "items/{{id}}"; status {{ 2xx => true, 404 => false }} }}',
            f'GET outcome_{i}(id: u64) -> Result<Item, Problem> {{ path "outcomes/{{id}}"; status {{ 409 {{ body }} => Err(body) }} }}',
            f'POST template_{i}(label: String, cursor?: String) -> Item {{ path "templates"; body json {{ "label" = "{{label}}/{{cursor}}"; }} }}',
        ][i % 6])
    if wide:
        names = [f'input_{i}' for i in range(200)]
        inputs = ', '.join(f'{name}: u32' for name in names)
        entries = '; '.join(names) + ';'
        endpoints = [f'GET wide({inputs}) -> Item {{ query {{ {entries} }} }}']
    description = 'concord::api! { pub client Mixed { base "https://mixed.test"; read Json; problem Problem;\n' + '\n'.join(endpoints) + '\n} }\n'
    (root / 'src/description.rs').write_text(description)
    (root / 'src/lib.rs').write_text('''#![deny(missing_docs)]
//! A mixed generated client used only for compile measurements.
use concord::{Json, json, by_offset_limit};
/// An inert item.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Item { id: u64 }
/// An inert problem.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Problem { message: String }
include!("description.rs");
''')
    if wide or count == 0:
        p = root / 'src/lib.rs'
        p.write_text(p.read_text().replace('use concord::{Json, json, by_offset_limit};', 'use concord::Json;'))
    return root


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    args = parser.parse_args()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    roots = [(str(count), prepare(count)) for count in [100, 500]] + [("wide200", prepare(1, wide=True))]
    if not args.run:
        print('Prepared 100/500 endpoints and one endpoint with 200 inputs; no Cargo command was run.')
        return
    env = os.environ.copy()
    env['CARGO_BUILD_JOBS'] = '4'
    env['CARGO_TARGET_DIR'] = str(API / 'target')
    # Warm only dependencies and a zero-endpoint client, outside the measurements.
    warm = prepare(0)
    for action in ['check', 'build']:
        with (OUTPUT / f'warm-{action}.log').open('w') as log:
            subprocess.run(['cargo', action, '--offline', '--manifest-path', str(warm / 'Cargo.toml')], env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    results = []
    for count, root in roots:
        for action in ['check', 'build']:
            measurement = OUTPUT / f'{action}-{count}.time'
            command = ['/usr/bin/time', '-v', '-o', str(measurement), 'cargo', action, '--offline', '--manifest-path', str(root / 'Cargo.toml')]
            started = time.monotonic()
            with (OUTPUT / f'{action}-{count}.log').open('w') as log:
                result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
            results.append({'endpoints': count, 'action': action, 'seconds': time.monotonic()-started, 'exit': result.returncode, 'time_file': str(measurement), 'peak_kib': int(re.search(r'Maximum resident set size \(kbytes\): (\d+)', measurement.read_text()).group(1))})
            (OUTPUT / 'results.json').write_text(json.dumps(results, indent=2)+'\n')
            print(json.dumps(results[-1]), flush=True)
            if result.returncode:
                raise SystemExit(result.returncode)
        env['CONCORD_MEASURE_INPUT'] = str(root / 'src/description.rs')
        env['CONCORD_MEASURE_OUTPUT'] = str(OUTPUT / f'expansion-{count}.json')
        command = ['cargo', 'test', '--offline', '-p', 'concord-macros', '--lib', 'write_tests::c1_16_measure_macro_expansion_once', '--', '--ignored', '--exact']
        with (OUTPUT / f'expansion-{count}.log').open('w') as log:
            result = subprocess.run(command, cwd=API, env=env, stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            raise SystemExit(result.returncode)
        print((OUTPUT / f'expansion-{count}.json').read_text().strip(), flush=True)
    print((OUTPUT / 'results.json').read_text(), flush=True)


if __name__ == '__main__':
    main()
