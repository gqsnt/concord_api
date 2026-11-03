"""Reconcile actual logs and finalize narrow review receipts; never modify source."""
from pathlib import Path
import hashlib
import json
import os
import re
import stat
import subprocess

ROOT = Path.cwd()
BASE = ROOT / 'target/probes'
all_runs = json.loads((BASE/'execution-results.json').read_text()) + json.loads((BASE/'additional-execution-results.json').read_text())
assert all(r['exit_code'] == r['expected_exit'] for r in all_runs)

def hint_receipt(package, label):
    directory = BASE/package
    expected = json.loads((directory/'expected.json').read_text())
    lines = (directory/'src/lib.rs').read_text().splitlines()
    header_lines = {e['case']: next(i for i, line in enumerate(lines, 1) if re.match(r'(?:pub )?mod '+re.escape(e['case'])+r' \{', line)) for e in expected}
    messages = []
    for line in (BASE/f'{package}-{label}.txt').read_text().splitlines():
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue
        if row.get('reason') == 'compiler-message' and row['message']['level'] == 'error':
            messages.append(row['message'])
    assert len(messages) == len(expected), (package, label, len(messages))
    found = {}
    for msg in messages:
        assert '`via` was removed:' in msg['message'], msg
        span = next(s for s in msg['spans'] if s['is_primary'])
        name = max((k for k,v in header_lines.items() if v <= span['line_start']), key=header_lines.get)
        assert name not in found, name
        expected_row = next(e for e in expected if e['case'] == name)
        assert '`'+expected_row['snippet']+'`' in msg['message'], (name, msg['message'])
        found[name] = {'expected_snippet': expected_row['snippet'], 'observed_message': msg['message'], 'line': span['line_start']}
    assert set(found) == set(header_lines)
    return found

hint_results = {}
test_results = {}
for label in ['pinned', 'stable', 'msrv']:
    hint_results[label] = {package: hint_receipt(package, label) for package in ['hints','renamed-hints']}
    test_results[label] = {}
    for package, count in [('migration',4),('matrix',38),('renamed',1)]:
        text = (BASE/f'{package}-{label}.txt').read_text()
        assert f'test result: ok. {count} passed; 0 failed;' in text
        test_results[label][package] = count
    assert 'expected identifier, found keyword `type`' in (BASE/f'keyword_suggested-{label}.txt').read_text()
    assert '`self::type b: String`' in (BASE/f'keyword_leftover-{label}.txt').read_text()

suite = (BASE/'formats-ui-pinned.txt').read_text()
assert 'test migrated_marker_paths_behave_like_the_words ... ok' in suite
assert 'test result: ok. 27 passed; 0 failed;' in suite
assert 'test result: ok. 2 passed; 0 failed;' in suite
ui_cases = re.findall(r'^test (tests/\S+\.rs) \.\.\. ok$', suite, re.M)
summary = {'toolchains': {'stable':'1.99.0','pinned':'1.97.1','msrv':'1.85.1'},
           'test_results': test_results,
           'matrix_per_toolchain': {'total':38,'automatic':35,'manual_self_qualification':3},
           'hint_diagnostics_per_toolchain': {'matrix':114,'raw_json':3,'raw_keyword':3,'renamed':8,'total':128},
           'repository_suite_once': {'toolchain':'1.97.1','formats':27,'ui_harnesses':2,'ui_cases':len(ui_cases)},
           'script_self_test_cases':11,'all_command_exit_codes_as_expected':True,
           'new_finding':'F3e: raw keyword r#type is suggested as invalid self::type; self::r#type compiles',
           'semantic_control_limit':'Explicit current-format controls; no older library accepting via was compiled.'}
(BASE/'hint-reconciliation.json').write_text(json.dumps(hint_results,indent=2)+'\n')
(BASE/'test-summary.json').write_text(json.dumps(summary,indent=2)+'\n')

triage = {'scope':'Narrow executable migration and leftover-via re-check of original d6b209f; not a full audit.',
          'verdict':'REVISE',
          'findings':[
              {'id':'F3','status':'RESOLVED','evidence':'codec::Text remains a path; original qualified reproducer and paired CUSTOM request/response observations pass on all three toolchains.'},
              {'id':'F3a','status':'RESOLVED','evidence':'Scoped, cfg-disabled and commented Concord imports do not affect migration. Four original reproducers plus expanded paired matrix pass.'},
              {'id':'F3b','status':'RESOLVED','evidence':'Local concord modules, import alias named concord, and macro re-export shadow retain custom codec body, Content-Type and decoded response.'},
              {'id':'F3c','status':'RESOLVED','evidence':'Original text/json/bytes cases are byte-for-byte untouched, reported, and rejected as leftover via. Applying self:: paths matches explicit custom controls. Raw r#json is also untouched and its suggestion compiles. Separate raw-keyword hint defect is F3e.'},
              {'id':'F3d','status':'RESOLVED','evidence':'All four archived renamed absolute-path compiler probes pass; matrix absolute body/response cases pass, as does repository migrated_marker_paths_behave_like_the_words.'},
              {'id':'F3e','severity':'P3','status':'NOT RESOLVED','title':'Migration guidance drops a required raw-identifier prefix',
               'locations':['scripts/migrate_via.py:46','scripts/migrate_via.py:55','crates/concord-macros/src/syntax.rs:168'],
               'input':'type r#type = concord::Text; concord::api! { pub client A { base "http://127.0.0.1"; POST p(body b: String via r#type); } }',
               'observed':'CLI leaves input unchanged and reports self::type. Compiler also suggests self::type b: String. Applying that hint fails: expected identifier, found keyword type.',
               'control':'self::r#type b: String compiles; raw r#json controls compile. Same result on Rust 1.99.0, 1.97.1 and 1.85.1. Body/optional/output diagnostics all drop r#.',
               'impact':'Invalid manual migration advice for keyword-named downstream codecs; no silent wire change. Preserve r# when qualifying the original identifier.',
               'reproduction':'python3 scripts/migrate_via.py target/probes/raw-keyword/keyword-migrated.rs; cargo +1.85 check --locked --offline --manifest-path target/probes/raw-keyword/Cargo.toml --bin keyword_leftover; compare keyword_suggested (fails) and keyword_corrected (passes).'}],
          'personally_executed':['All commands in execution-results.json and additional-execution-results.json','Current script applied to archived originals; 38 paired wire checks and four original reproducers on 3 toolchains','128 leftover-via diagnostics per toolchain, matched to source module and expected snippet','Raw-keyword suggested-edit failure and corrected-edit positive controls','Current source-tree provenance, baseline/final hashes and process cleanup'],
          'inspected_only':['Historical F1/F2/F4 dispositions and prior reviewers’ execution claims','Archived generator and prior report as inputs; their old outputs are not fresh evidence'],
          'not_run':['Full workspace verification','Benchmarks','Packaging checks','Nightly','An older library accepting via']}
(BASE/'triage.json').write_text(json.dumps(triage,indent=2)+'\n')

baseline = json.loads((BASE/'baseline-hashes.json').read_text())
changed = [p for p,h in baseline.items() if not (ROOT/p).is_file() or hashlib.sha256((ROOT/p).read_bytes()).hexdigest()!=h]
manifest = json.loads((ROOT/'.concord-review.json').read_text())
origin_changed=[]
for p,e in manifest['files'].items():
    data=(ROOT/p).read_bytes()
    blob=hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
    mode='100755' if (ROOT/p).stat().st_mode & stat.S_IXUSR else '100644'
    if blob!=e['blob'] or mode!=e['mode']:origin_changed.append(p)
active=[]
for proc in Path('/proc').iterdir():
    if not proc.name.isdigit() or int(proc.name)==os.getpid(): continue
    try:
        exe=os.readlink(proc/'exe'); args=(proc/'cmdline').read_bytes().split(b'\0'); cwd=os.readlink(proc/'cwd')
    except (OSError,PermissionError):continue
    # Check executable test processes and the task driver, excluding this receipt command's shell.
    if exe.startswith(str(ROOT/'target/')) or any(a.endswith(b'/run_recheck.py') or a==b'target/probes/run_recheck.py' for a in args):
        active.append({'pid':int(proc.name),'exe':exe,'cwd':cwd})
status=subprocess.check_output(['git','status','--short','--untracked-files=normal'],text=True)
integrity={'snapshot_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
           'baseline_tracked_files':len(baseline),'baseline_changed':changed,
           'original_files':len(manifest['files']),'original_changed':origin_changed,
           'git_status':status,'active_probe_processes':active,
           'servers':'Independent listeners are owned by joined threads; test processes have exited.',
           'external_services_contacted':False,'push_or_publish':False,'global_setting_changes':False,'sandbox_bypass':False}
(BASE/'source-integrity.json').write_text(json.dumps(integrity,indent=2)+'\n')
assert not changed and not origin_changed and not active and not status
print(json.dumps(summary,indent=2))
print(json.dumps(integrity,indent=2))
