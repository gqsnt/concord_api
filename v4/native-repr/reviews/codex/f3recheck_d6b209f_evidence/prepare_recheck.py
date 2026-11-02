"""Adapt the archived 38-case generator to the deliberately manual F3c migration."""
from pathlib import Path
import json
import re
import subprocess

ROOT = Path.cwd()
BASE = ROOT / 'target/probes'
P = BASE / 'matrix'
cases = json.loads((P / 'cases.json').read_text())
receipt = []
for c in cases:
    name, path = c['name'], c['path']
    original = (P / f'original/{name}.rs').read_text()
    migrated = (P / f'migrated/{name}.rs').read_text()
    ty = 'Vec<u8>' if c['binary'] else 'String'
    if name.startswith('bare_lowercase_'):
        assert original == migrated, name
        src = (P / f'src/{name}.rs').read_text()
        src = src.replace(f'body b: {ty} via {path}', f'self::{path} b: {ty}')
        src = src.replace(f'body b?: {ty} via {path}', f'self::{path} b?: {ty}')
        src = src.replace(f'-> {ty} via {path}', f'-> self::{path} {ty}')
        assert ' via ' not in src
        (P / f'src/{name}.rs').write_text(src)
        mode = 'script preserves original; author applies self:: path for wire check'
    else:
        assert ' via ' not in migrated, name
        assert migrated.count(path) >= 3, name
        mode = 'automatic migration'
    receipt.append({'case': name, 'mode': mode})
(BASE / 'migration-assertions.json').write_text(json.dumps(receipt, indent=2)+'\n')

# Isolate body, optional body, and response so each macro emits its own via diagnostic.
H = BASE / 'hints'
(H / 'src').mkdir(parents=True, exist_ok=True)
(H / 'Cargo.toml').write_text((P / 'Cargo.toml').read_text().replace('f3check-matrix', 'f3check-hints'))
preamble = (P / 'src/lib.rs').read_text().split('\npub type Text =')[0].replace('mod wire;', '')
preamble += '\npub type Text = codec::Text;\n'
lib, expected = preamble, []
for c in cases:
    name, path = c['name'], c['path']
    ty = 'Vec<u8>' if c['binary'] else 'String'
    original = (P / f'original/{name}.rs').read_text()
    found = re.search(r'([\w:]+)! \{', original)
    setup = original[:found.start()]
    macro = found.group(1)
    for site in ['body', 'optional', 'output']:
        module = name + '_' + site
        endpoint = {
            'body': f'POST p(body b: {ty} via {path});',
            'optional': f'POST p(body b?: {ty} via {path});',
            'output': f'GET p() -> {ty} via {path};',
        }[site]
        text = setup + f'{macro}! {{ pub client Hint {{ base "http://127.0.0.1"; {endpoint} }} }}\n'
        if c['depth']:
            text += '}\n'
        lib += f'pub mod {module} {{\n{text}}}\n'
        replacement = 'self::'+path if name.startswith('bare_lowercase_') else path
        snippet = f'-> {replacement} {ty}' if site == 'output' else f'{replacement} b'+('?' if site == 'optional' else '')+f': {ty}'
        expected.append({'case': module, 'path': path, 'snippet': snippet})

# Raw non-keyword and raw keyword spellings exercise the same F3c hint path.
for path in ['r#json', 'r#type']:
    for site in ['body', 'optional', 'output']:
        name = path[2:] + '_' + site
        endpoint = {'body': f'POST p(body b: String via {path});',
                    'optional': f'POST p(body b?: String via {path});',
                    'output': f'GET p() -> String via {path};'}[site]
        lib += f'pub mod raw_{name} {{ type {path} = crate::codec::Text; crate::c4::api! {{ pub client Hint {{ base "http://127.0.0.1"; {endpoint} }} }} }}\n'
        replacement = 'self::'+path[2:]
        snippet = f'-> {replacement} String' if site == 'output' else f'{replacement} b'+('?' if site == 'optional' else '')+': String'
        expected.append({'case': 'raw_'+name, 'path': path, 'snippet': snippet})
(H / 'src/lib.rs').write_text(lib)
(H / 'expected.json').write_text(json.dumps(expected, indent=2)+'\n')

# Small independently runnable regression: a raw keyword type is legal; its raw prefix is needed.
E = BASE / 'raw-keyword'
(E / 'src/bin').mkdir(parents=True, exist_ok=True)
(E / 'Cargo.toml').write_text((P / 'Cargo.toml').read_text().replace('f3check-matrix', 'f3check-raw-keyword'))
for path, name in [('r#type', 'keyword'), ('r#json', 'raw_json')]:
    source = f'''#![allow(non_camel_case_types)]
type {path} = concord::Text;
concord::api! {{ pub client A {{ base "http://127.0.0.1";
    POST p(body b: String via {path});
}} }}
fn main() {{}}
'''
    (E / f'{name}-original.rs').write_text(source)
    migrated = E / f'{name}-migrated.rs'
    migrated.write_text(source)
    result = subprocess.run(['python3', 'scripts/migrate_via.py', str(migrated)], text=True, capture_output=True, check=True)
    assert migrated.read_text() == source
    (E / f'{name}-migration.txt').write_text(result.stdout+result.stderr)
    for variant, replacement in [('leftover', f'body b: String via {path}'),
                                 ('suggested', f'self::{path[2:]} b: String'),
                                 ('corrected', f'self::{path} b: String')]:
        (E / f'src/bin/{name}_{variant}.rs').write_text(source.replace(f'body b: String via {path}', replacement))
print(f'Prepared 38 paired wire cases (35 automatic, 3 manual), {len(expected)} hint cases, raw-keyword reproducer.')
