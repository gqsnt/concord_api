#!/usr/bin/env python3
"""Verify whole descriptions from the guide, overview, README, and API examples."""
import argparse
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'tests/real-apis/fixtures/descriptions'


def mask(source):
    """Hide comments and literals while preserving offsets and code delimiters."""
    chars = list(source)
    at = 0
    while at < len(source):
        end = None
        if source.startswith('//', at):
            end = source.find('\n', at)
            if end < 0:
                end = len(source)
        elif source.startswith('/*', at):
            depth, cursor = 1, at + 2
            while depth and cursor < len(source):
                if source.startswith('/*', cursor):
                    depth += 1
                    cursor += 2
                elif source.startswith('*/', cursor):
                    depth -= 1
                    cursor += 2
                else:
                    cursor += 1
            end = cursor
        elif match := re.match(r'(?:br|r)(#*)"', source[at:]):
            closing = '"' + match[1]
            close = source.find(closing, at + match.end())
            end = len(source) if close < 0 else close + len(closing)
        elif source[at] == '"':
            cursor = at + 1
            while cursor < len(source):
                if source[cursor] == '\\':
                    cursor += 2
                elif source[cursor] == '"':
                    cursor += 1
                    break
                else:
                    cursor += 1
            end = cursor
        elif match := re.match(r"'(?:\\.|[^'\\])'", source[at:]):
            end = at + match.end()
        if end is not None:
            for index in range(at, end):
                if chars[index] != '\n':
                    chars[index] = ' '
            at = end
        else:
            at += 1
    return ''.join(chars)


def descriptions(block):
    visible = mask(block)
    macros = list(re.finditer(r'\bconcord\s*::\s*api\s*!\s*\{', visible))
    clients = list(re.finditer(r'\b(?:pub\s+)?client\s+\w+', visible))
    candidates = sorted([(m, True) for m in macros] + [(m, False) for m in clients],
                        key=lambda item: item[0].start())
    consumed = 0
    for match, is_macro in candidates:
        if match.start() < consumed:
            continue
        opening = visible.find('{', match.start())
        if opening < 0:
            continue
        depth, end = 1, opening + 1
        while depth and end < len(visible):
            depth += (visible[end] == '{') - (visible[end] == '}')
            end += 1
        if depth:
            continue  # A grammar fragment, not a whole description.
        start = opening + 1 if is_macro else match.start()
        stop = end - 1 if is_macro else end
        consumed = end
        source = block[start:stop]
        if "…" in visible[start:stop] or re.search(r"//\s*base, read, endpoints", source):
            continue  # An explicitly omitted body is a fragment.
        yield source.strip() + '\n'


def formatted(source):
    return subprocess.run(['rustfmt', '--edition', '2021'], input=source, text=True,
                          stdout=subprocess.PIPE, check=True).stdout


def expected():
    pages = [ROOT / 'README.md', ROOT / 'docs/reference/overview.md',
             *sorted((ROOT / 'docs/guide').glob('*.md')),
             *sorted((ROOT / 'docs/examples').glob('*.md'))]
    contents, inventory, modules, types = {}, [], [], []
    for page in pages:
        relative = page.relative_to(ROOT).as_posix()
        prefix = relative.removeprefix('docs/').removesuffix('.md').replace('/', '_').lower()
        count = 0
        blocks = re.findall(r'^```[^\n]*\n(.*?)^```', page.read_text(), re.M | re.S)
        for block_number, block in enumerate(blocks, 1):
            for source in descriptions(block):
                count += 1
                name = f'{prefix}_{count:03}'
                client = re.search(r'\bclient\s+(\w+)', mask(source))[1]
                fixture = f'tests/real-apis/fixtures/descriptions/{name}.api'
                contents[fixture] = source
                if relative.startswith('docs/examples/'):
                    api = page.stem
                    compiled = f'tests/real-apis/src/{api}/description.rs'
                    if compiled in contents:
                        raise ValueError(f'{relative}: another whole description needs its own compilation module')
                    contents[compiled] = block.strip() + '\n'
                    types.append(f'::std::any::type_name::<crate::{api}::{client}>()')
                else:
                    compiled = f'tests/real-apis/src/guide/{name}.rs'
                    contents[compiled] = formatted('concord::api! {\n' + source + '}\n')
                    module = name.replace('-', '_')
                    modules.append(f'''#[doc(hidden)]
mod {module} {{
    use super::support::*;
    include!("{name}.rs");
    pub(super) fn compiled() -> &'static str {{ ::std::any::type_name::<{client}>() }}
}}''')
                    types.append(f'{module}::compiled()')
                inventory.append({'page': relative, 'block': block_number,
                                  'fixture': fixture, 'compiled': compiled, 'client': client})
    registry = '//! Compiled descriptions from the current documentation.\n#![allow(dead_code, unused_imports)]\npub mod support;\n'
    registry += '\n'.join(modules)
    registry += f'''\n/// The concrete client types emitted by every documented description.
pub fn compiled_descriptions() -> [&'static str; {len(types)}] {{
    [{', '.join(types)}]
}}\n'''
    contents['tests/real-apis/src/guide/mod.rs'] = formatted(registry)
    contents['tests/real-apis/fixtures/descriptions/manifest.json'] = json.dumps(inventory, indent=2) + '\n'
    return contents, inventory


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args(argv)
    contents, inventory = expected()
    if args.check:
        different = {name for name, text in contents.items()
                     if not (ROOT / name).is_file() or (ROOT / name).read_text() != text}
        actual = {p.relative_to(ROOT).as_posix() for p in FIXTURES.iterdir() if p.is_file()}
        different.update(actual - contents.keys())
        actual_compiled = {p.relative_to(ROOT).as_posix()
                           for p in (ROOT / 'tests/real-apis/src/guide').glob('*.rs')
                           if p.name != 'support.rs'}
        different.update(actual_compiled - contents.keys())
        if different:
            print('DOC EXAMPLES: changed fixtures: ' + ', '.join(sorted(different)))
            return 1
        print(f'DOC EXAMPLES: {len(inventory)} whole descriptions match their compiled sources')
        return 0
    output = ROOT / 'target/doc-examples'
    for name, text in contents.items():
        destination = output / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text)
    print(f'DOC EXAMPLES: prepared {len(inventory)} whole descriptions in {output}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
