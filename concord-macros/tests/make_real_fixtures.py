"""Prepare diagnostic sources from complete API copies; never write stderr.

The manifest records one diagnostic fault and its repair for each case. The
compiled Rust table lets the tests reconstruct each mistake independently and
check the literal repair up to code generation. --check changes no fixtures.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent


def outputs():
    cases = json.loads((ROOT / 'diagnostics/api/cases.json').read_text())
    rust = [
        "//! Generated diagnostic repair cases for complete API descriptions.",
        '#[allow(clippy::type_complexity)]',
        'pub(super) const CASES: &[(&str, &str, u8, &[(&str, &str, usize)])] = &[',
    ]
    quote = lambda text: json.dumps(text, ensure_ascii=False)
    for case in cases:
        source = (ROOT / 'snapshots/resolved' / case['base']).read_text()
        edits = []
        for edit in case['edits']:
            old, new = edit[:2]
            occurrence = edit[2] if len(edit) == 3 else 0
            offset = -1
            for _ in range(occurrence + 1):
                offset = source.index(old, offset + 1)
            source = source[:offset] + new + source[offset + len(old):]
            edits.append(f'({quote(old)}, {quote(new)}, {occurrence})')
        yield ROOT / 'diagnostics/api' / case['fixture'], source.rstrip() + '\nfn main() {}\n\ninclude!("../../support/models.rs");\n'
        rust.append(f'    ({quote(case["fixture"])}, {quote(case["base"])}, '
                    f'{case["number"]}, &[{", ".join(edits)}]),')
    rust.append('];')
    formatted = subprocess.run(['rustfmt', '--edition', '2021'],
                               input='\n'.join(rust) + '\n', text=True,
                               capture_output=True, check=True).stdout
    yield ROOT / 'unit/repair_cases.rs', formatted


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    different = []
    for path, content in outputs():
        if args.check:
            if path.read_text() != content:
                different.append(str(path.relative_to(ROOT)))
        else:
            output = ROOT.parents[1] / "target" / "real-fixtures" / path.relative_to(ROOT)
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(content)
    if different:
        raise SystemExit('fixtures differ: ' + ', '.join(different))
    print('97 real-description fixtures and their compiled mutation table verified')


if __name__ == '__main__':
    main()
