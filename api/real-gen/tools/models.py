"""Regenerate inert nominal model declarations; descriptions are never rewritten."""
from pathlib import Path
import json
import re
import sys
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT.parent / 'tools'))
from matrix import rust_tokens

known = set('String Vec Option Result HashMap Some None Ok Err Self Json Text Stream Binary Base Body ByteStream Secret Value Bytes Duration Xml WatchLines Stub ObserveQuota Signed AsJson Quota Cooldown TraceRequests AttemptTrace KubeWarnings'.split())
for module, page in json.loads((ROOT / 'inventory.json').read_text()):
    root = ROOT / 'src' / module
    description = ROOT / 'cases/fail/telegram.rs' if module == 'telegram' else root / 'description.rs'
    text = description.read_text()
    client = re.search(r'(?m)^\s*(?:pub )?client\s+(\w+)', text).group(1)
    problem = re.search(r'\bproblem\s+([\w:]+)', text).group(1)
    tokens = [t.text for t in rust_tokens(text)]
    names = {t for t in tokens if re.fullmatch('[A-Z][a-zA-Z0-9_]*', t) and not t.isupper()} - known - {client}
    generic = {t for i, t in enumerate(tokens[:-1]) if t in names and tokens[i+1] == '<'}
    models = [f'type ProbeProblem = {problem};']
    for name in sorted(names):
        if name == 'Reply':
            models += ['/// The typed success envelope.', 'pub type Reply<T> = StubReply<T>;']
        elif name == 'MaybeDeleted':
            models += ['/// The live/deleted envelope.', 'pub type MaybeDeleted<T> = StubMaybe<T>;']
        else:
            models.append(f'model!({name}' + ('<T>' if name in generic else '') + ');')
    for name in ['Quota', 'Cooldown']:
        models += [f'/// Inert middleware `{name}`.', f'pub type {name} = Stub;']
    (root / 'types.rs').write_text('\n'.join(models) + '\n')
    include = '\n// The original page is in cases/fail.\n' if module == 'telegram' else '\ninclude!("description.rs");\n'
    (root / 'mod.rs').write_text(f'//! The unedited {page} description.\n#![allow(non_upper_case_globals, non_snake_case)]\ninclude!("../scaffold.rs");\ninclude!("types.rs");'+include)
