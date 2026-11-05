"""Materialize only the pre-header-fix extras source under the allowed scratch tree."""
from pathlib import Path
import subprocess, tomllib

root = Path(__file__).resolve().parents[2]
dest = root / 'review/sol-recheck/baseline-extras'
dest.mkdir(exist_ok=True)
commit = 'e27a0958^'
files = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', commit, 'crates/concord-extras/src'], cwd=root, text=True).splitlines()
for path in files:
    target = dest / Path(path).relative_to('crates/concord-extras')
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(subprocess.check_output(['git', 'show', f'{commit}:{path}'], cwd=root))
manifest = subprocess.check_output(['git', 'show', f'{commit}:crates/concord-extras/Cargo.toml'], cwd=root, text=True)
manifest = manifest.split('[dev-dependencies]')[0]
manifest = manifest.replace('name = "concord-extras"', 'name = "concord-extras-before"')
workspace = tomllib.loads((root/'Cargo.toml').read_text())
for key, value in workspace['workspace']['package'].items():
    import json
    import re
    manifest = re.sub(rf'(?m)^{re.escape(key)}\.workspace = true$', f'{key} = {json.dumps(value)}', manifest)
for key, value in workspace['workspace']['dependencies'].items():
    value = value if isinstance(value, str) else value['version']
    manifest = manifest.replace(f'{key}.workspace = true', f'{key} = "{value}"')
    manifest = manifest.replace(f'{key} = {{ workspace = true,', f'{key} = {{ version = "{value}",')
manifest = manifest.replace('path = "../concord-core"', f'path = "{root}/crates/concord-core"')
(dest/'Cargo.toml').write_text(manifest)

# The original test's measurement mechanics, with an additional lazy payload route and a
# selectable baseline library. Input size is a process argument, allowing a scaling control.
source = (root/'crates/concord-extras/tests/csv_write_memory.rs').read_text()
source = source[source.index('use std::convert'):]
source = source.replace('use concord_extras::', 'use extras::')
source = source.replace('const FIELD: usize = 16 * 1024 * 1024;', 'fn field() -> usize { std::env::args().nth(2).unwrap().parse::<usize>().unwrap() * 1024 * 1024 }')
start = source.index('const TEST:')
end = source.index('/// One route', start)
source = source[:start] + source[end:]
source = source.replace('FIELD', 'field()')
marker = '        // ... inferred from the row\'s field name, ...'
source = source.replace(marker, '''        "payload columns, lazy" => {
            let source = from_iter([Ok::<_, Infallible>((1,))]);
            let rows = StreamRows::new(source).with_columns(["h".repeat(field())]);
            growth(|| {
                let upload = writer.upload(rows).unwrap();
                assert_eq!(refused_lazily(upload), (HEADER_REFUSED, Some(64)));
            })
        }
''' + marker)
prefix = '''#[cfg(feature = "before")]
use concord_extras_before as extras;
#[cfg(not(feature = "before"))]
use concord_extras as extras;
'''
source += '\nfn main() { let route = std::env::args().nth(1).unwrap(); let kib = measure(&route); println!("{route}; size={} MiB; growth={kib} KiB", field()/1024/1024); }\n'
(root/'review/sol-recheck/probes/src/bin/header_memory.rs').write_text(prefix+source)
