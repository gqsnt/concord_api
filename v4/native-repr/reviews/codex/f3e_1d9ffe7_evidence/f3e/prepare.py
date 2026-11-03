import hashlib,json,os,pathlib,stat,subprocess
ROOT=pathlib.Path.cwd()
OUT=ROOT/'target/probes/f3e'
def save(name,data): (OUT/name).write_text(json.dumps(data,indent=2)+'\n')
def blob(data): return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
manifest=json.loads((ROOT/'.concord-review.json').read_text())
tracked=subprocess.check_output(['git','ls-files','-z'],text=True).split('\0')[:-1]
files={p:{'sha256':hashlib.sha256((ROOT/p).read_bytes()).hexdigest(),'mode':stat.S_IMODE((ROOT/p).stat().st_mode)} for p in tracked}
checks=[]
for p,expected in manifest['files'].items():
 data=(ROOT/p).read_bytes()
 if blob(data)!=expected['blob']: checks.append({'path':p,'actual':blob(data),'expected':expected['blob']})
save('baseline-integrity.json',{'snapshot_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'original_commit':manifest['original_commit'],'original_tree':manifest['original_tree'],'manifest_original_file_count':len(manifest['files']),'tracked_file_count':len(tracked),'manifest_mismatches':checks,'files':files})
assert not checks,checks
thread=os.environ['CODEX_THREAD_ID']
paths=list(pathlib.Path('/home/canarit/.codex/sessions').rglob('*'+thread+'*'))
meta=[]; contexts=[]
for p in paths:
 for line in p.read_text().splitlines():
  d=json.loads(line); q=d.get('payload',{})
  if d.get('type')=='session_meta': meta.append({'timestamp':d['timestamp'],**{k:q.get(k) for k in ['id','cwd','originator','cli_version','source','model_provider']}})
  if d.get('type')=='turn_context': contexts.append({'timestamp':d['timestamp'],**{k:q.get(k) for k in ['cwd','approval_policy','sandbox_policy','model','effort','collaboration_mode']}})
versions={tc:{c:subprocess.check_output([c,'+'+tc,'-V'],text=True).strip() for c in ['rustc','cargo']} for tc in ['stable','1.85']}
setting_files=['/home/canarit/.codex/config.toml','/home/canarit/.rustup/settings.toml',os.environ['CARGO_HOME']+'/config.toml']
save('environment-receipt.json',{'requested':{'mode':'execute','model':'gpt-6.1-sol','effort':'xhigh'},'session_meta':meta,'observed_turn_context':contexts,'runtime_metadata_paths':[str(p) for p in paths],'environment':{k:os.environ.get(k) for k in ['CARGO_HOME','CARGO_TARGET_DIR','RUSTUP_HOME','RUSTUP_TOOLCHAIN','CODEX_THREAD_ID','CODEX_MODEL','CODEX_MODEL_REASONING_EFFORT']},'toolchain_versions':versions,'settings_file_hashes':{p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest() if pathlib.Path(p).exists() else None for p in setting_files},'execution_policy':'All cargo commands --offline; checks/tests --locked. All probes in target/probes. No external services, global-setting changes, push, publish, or sandbox bypass. No agents delegated.'})
PROBE=OUT/'raw-keyword'
(PROBE/'src/bin').mkdir(parents=True,exist_ok=True)
(PROBE/'Cargo.toml').write_text('[package]\nname = "f3e-raw-keyword"\nversion = "0.0.0"\nedition = "2024"\nrust-version = "1.85"\n[dependencies]\nconcord = { path = "'+str(ROOT/'crates/concord')+'", default-features = false }\n[workspace]\n')
historical=ROOT/'v4/native-repr/reviews/codex/f3recheck_d6b209f_evidence/raw-keyword/src/bin'
for name in ['keyword_leftover','keyword_corrected']:
 (PROBE/'src/bin'/f'{name}.rs').write_text((historical/f'{name}.rs').read_text())
(OUT/'historical-keyword_suggested.rs').write_text((historical/'keyword_suggested.rs').read_text())
cases=[]
for spelling in ['r#type','r#text','r#json','r#bytes','r#form','r#csv','text','json','bytes','form','csv']:
 for site in ['body','optional','output']:
  ident=spelling.replace('r#','raw_')+'_'+site
  old={'body':f'body b: String via {spelling}','optional':f'body b?: String via {spelling}','output':f'-> String via {spelling}'}[site]
  expected={'body':f'self::{spelling} b: String','optional':f'self::{spelling} b?: String','output':f'-> self::{spelling} String'}[site]
  endpoint=f'GET p() {old};' if site=='output' else f'POST p({old});'
  source=f'pub mod {ident} {{ type {spelling} = concord::Text; concord::api! {{ pub client A {{ base "http://127.0.0.1"; {endpoint} }} }} }}'
  cases.append({'id':ident,'spelling':spelling,'site':site,'old':old,'expected':expected,'source':source})
(PROBE/'src/bin/hints.rs').write_text('#![allow(non_camel_case_types, dead_code)]\n'+'\n'.join(c['source'] for c in cases)+'\nfn main() {}\n')
format_cases=[]
for site in ['body','output']:
 ident='format_'+site
 endpoint='POST p(r#type b: String);' if site=='body' else 'GET p() -> r#type String;'
 source=f'pub mod {ident} {{ type r#type = concord::Text; concord::api! {{ pub client A {{ base "http://127.0.0.1"; {endpoint} }} }} }}'
 format_cases.append({'id':ident,'site':site,'source':source})
(PROBE/'src/bin/format_errors.rs').write_text('#![allow(non_camel_case_types, dead_code)]\n'+'\n'.join(c['source'] for c in format_cases)+'\nfn main() {}\n')
save('cases.json',{'via':cases,'format':format_cases})
(OUT/'script-input.rs').write_text((PROBE/'src/bin/keyword_leftover.rs').read_text().replace('    POST p(body b: String via r#type);','    POST p(body b: String via r#type);\n    GET q() -> String via r#type;'))
print(json.dumps({'baseline_matches':not checks,'original_files':len(manifest['files']),'tracked_files':len(tracked),'versions':versions,'via_cases':len(cases),'format_cases':len(format_cases),'metadata_model':contexts[-1]['model'],'metadata_effort':contexts[-1]['effort']},indent=2))
