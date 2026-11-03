"""Mutate isolated source copies, asserting pass -> intended runtime failure -> restored pass."""
import json,os,pathlib,shutil,subprocess
root=pathlib.Path(__file__).resolve().parent
snapshot=root.parent.parent
copy=root/'mutation-source'
copy.mkdir(exist_ok=True)
for crate in ['concord','concord-macros']:
 shutil.copytree(snapshot/'crates'/crate,copy/'crates'/crate,dirs_exist_ok=True)
for name in ['Cargo.lock','README.md','LICENSE-APACHE','LICENSE-MIT']:
 if (snapshot/name).exists():shutil.copy2(snapshot/name,copy/name)
manifest=(snapshot/'Cargo.toml').read_text()
manifest=manifest.replace(next(l for l in manifest.splitlines() if l.startswith('members =')), 'members = ["crates/concord", "crates/concord-macros"]')
(copy/'Cargo.toml').write_text(manifest)
consumer=root/'mutation-consumer'
shutil.copytree(root/'acceptance',consumer,dirs_exist_ok=True)
manifest=(consumer/'Cargo.toml').read_text().replace('../../../crates/concord','../mutation-source/crates/concord')
(consumer/'Cargo.toml').write_text(manifest)
env=dict(os.environ,CARGO_TARGET_DIR=str(root.parent/'mutation-build'),CARGO_NET_OFFLINE='true')
cmd=['cargo','test','--offline','--locked','--manifest-path',str(consumer/'Cargo.toml'),'--test','contract']
logdir=root/'mutations';logdir.mkdir(exist_ok=True)
mutations=[
 ('clear_native_flag','crates/concord/src/__private.rs',
  'if sensitive {\n            value.set_sensitive(true);\n        }',
  'value.set_sensitive(sensitive);',
  'native_owners_clone_snapshots_flags_bytes_and_wire','NATIVE-SECRET'),
 ('insert_ct_without_body','crates/concord/src/__private.rs',
  'let mut headers = self.headers;',
  'let mut headers = self.headers;\n        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));',
  'format_presence_aliases_independent_defaults_lexical_and_media','body(&p).is_none()'),
 ('lose_inserted_ct_classification','crates/concord/src/__private.rs',
  'if self.meta.sensitive_content_type {',
  'if false {',
  'composed_content_type_and_sensitive_insertions_reach_wire','is_sensitive'),
 ('decode_no_output_under_default','crates/concord/src/__private.rs',
  'Decoder::new(meta, |_| Ok(()), true)',
  'Decoder::new(meta, |_| Err(BodyError::new("mutant decoder ran")), false)',
  'drain_typed_unit_string_alias_status_and_native_decoder','mutant decoder ran'),
]
results=[]
for name,file,old,new,test,needle in mutations:
 path=copy/file;original=path.read_text()
 assert original.count(old)==1,(name,original.count(old))
 result={'name':name,'test':test,'phases':[]}
 try:
  for phase,source in [('baseline',original),('mutant',original.replace(old,new)),('restored',original)]:
   path.write_text(source)
   p=subprocess.run(cmd+[test,'--','--exact','--nocapture'],env=env,text=True,capture_output=True)
   log=p.stdout+p.stderr;(logdir/f'{name}-{phase}.txt').write_text(log)
   ok=p.returncode==0 if phase!='mutant' else p.returncode!=0 and 'test result: FAILED.' in log and needle in log
   result['phases'].append({'phase':phase,'exit':p.returncode,'ok':ok})
   print(name,phase,'PASS' if ok else 'FAIL',p.returncode,flush=True)
   if not ok:print(log,flush=True)
 finally:path.write_text(original)
 result['restored_source']=path.read_text()==original
 results.append(result)
(root/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
assert all(r['restored_source'] and all(p['ok'] for p in r['phases']) for r in results)
