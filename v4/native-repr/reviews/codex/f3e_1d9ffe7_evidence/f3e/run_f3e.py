import hashlib,json,os,pathlib,re,subprocess,time,traceback
ROOT=pathlib.Path.cwd(); OUT=ROOT/'target/probes/f3e'; PROBE=OUT/'raw-keyword'
CASES=json.loads((OUT/'cases.json').read_text()); records=[]; reconciliation={}; failures=[]
def save(name,data): (OUT/name).write_text(json.dumps(data,indent=2)+'\n')
def run(name,args,tc=None,expected=0):
 env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',CARGO_NET_OFFLINE='true')
 if tc: env['CARGO_TARGET_DIR']=str(ROOT/'target/review-build'/('f3e-'+tc))
 print('START '+name,flush=True)
 start=time.monotonic()
 p=subprocess.run(args,cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 (OUT/(name+'.stdout.txt')).write_text(p.stdout); (OUT/(name+'.stderr.txt')).write_text(p.stderr)
 rec={'name':name,'command':args,'cwd':str(ROOT),'cargo_target_dir':env.get('CARGO_TARGET_DIR'),'exit_code':p.returncode,'expected_exit_code':expected,'seconds':round(time.monotonic()-start,3),'stdout':name+'.stdout.txt','stderr':name+'.stderr.txt'}
 records.append(rec);save('execution-results.json',records)
 print(f'END {name}: exit={p.returncode}, expected={expected}, seconds={rec["seconds"]}',flush=True)
 if p.returncode!=expected: raise AssertionError(f'{name}: exit={p.returncode}, expected={expected}\n{p.stderr[-5000:]}\n{p.stdout[-3000:]}')
 return p

def messages(p):
 result=[]
 for line in p.stdout.splitlines():
  try: d=json.loads(line)
  except ValueError: continue
  if d.get('reason')=='compiler-message' and d['message']['level']=='error': result.append(d['message'])
 return result

def check(tc,binary,expected=0):
 return run(binary+'-'+tc,['cargo','+'+tc,'check','--locked','--offline','--manifest-path',str(PROBE/'Cargo.toml'),'--bin',binary,'--message-format=json'],tc,expected)

def suggestion(m):
 match=re.search(r'the format comes before the (?:name|type), `([^`]+)` \(the same type\)',m['message'])
 assert match,m['message']
 return match.group(1)

try:
 p=run('script-self-test',['python3','scripts/migrate_via.py','--self-test'])
 assert 'self-test: 11 cases ok' in p.stdout
 original=(OUT/'script-input.rs').read_bytes()
 p=run('script-raw-keyword',['python3','scripts/migrate_via.py',str(OUT/'script-input.rs')])
 assert (OUT/'script-input.rs').read_bytes()==original
 printed=re.findall(r'kept: `via r#type` \(write `([^`]+)` or the word yourself\)',p.stderr)
 assert printed==['self::r#type','self::r#type'],p.stderr
 assert re.search(r'^\s*0\s+',p.stdout),p.stdout
 script_source=original.decode().replace('body b: String via r#type',printed[0]+' b: String').replace('-> String via r#type','-> '+printed[1]+' String')
 (PROBE/'src/bin/script_suggested.rs').write_text(script_source)
 save('script-reconciliation.json',{'self_test_cases':11,'printed_suggestions':printed,'input_unchanged':True,'rewrite_count':0,'applied_source':'raw-keyword/src/bin/script_suggested.rs'})
 run('probe-lock',['cargo','+1.85','generate-lockfile','--offline','--manifest-path',str(PROBE/'Cargo.toml')],'1.85')
 for tc in ['stable','1.85']:
  try:
   p=run('macro-lib-'+tc,['cargo','+'+tc,'test','--locked','--offline','-p','concord-macros','--lib'],tc)
   assert 'syntax::tests::via_replacement_names_the_same_path ... ok' in p.stdout,p.stdout
   test_summary=re.search(r'test result: ok\. (\d+) passed; (\d+) failed;',p.stdout)
   assert test_summary,p.stdout
   recon={'macro_lib_tests_passed':int(test_summary.group(1))}
   p=check(tc,'keyword_leftover',101); errors=messages(p)
   assert len(errors)==1,errors
   s=suggestion(errors[0]); assert s=='self::r#type b: String',s
   source=(PROBE/'src/bin/keyword_leftover.rs').read_text().replace('body b: String via r#type',s)
   (PROBE/'src/bin/keyword_suggested.rs').write_text(source)
   (OUT/('keyword_suggested-'+tc+'.rs')).write_text(source)
   assert source==(PROBE/'src/bin/keyword_corrected.rs').read_text()
   check(tc,'keyword_suggested'); check(tc,'keyword_corrected')
   recon['keyword']={'compiler_suggestion':s,'applied_verbatim':True,'compiles':True,'identical_to_corrected_control':True}
   p=check(tc,'hints',101); errors=messages(p)
   assert len(errors)==len(CASES['via']),(len(errors),len(CASES['via']))
   actual=[]; fixed=[]
   for i,c in enumerate(CASES['via']):
    matching=[m for m in errors if any(span.get('is_primary') and span.get('line_start')==i+2 and span['file_name'].endswith('hints.rs') for span in m['spans'])]
    assert len(matching)==1,(c['id'],matching)
    m=matching[0]; assert '`via` was removed:' in m['message'],m['message']
    s=suggestion(m); assert s==c['expected'],(c['id'],s,c['expected'])
    assert all(span['text'][0]['text'][span['column_start']-1:span['column_end']-1]=='via' for span in m['spans'] if span.get('is_primary')),m['spans']
    fixed.append(c['source'].replace(c['old'],s))
    actual.append({'id':c['id'],'suggestion':s,'diagnostic':m['message'],'primary_span_is_via':True})
   (PROBE/'src/bin/hints_suggested.rs').write_text('#![allow(non_camel_case_types, dead_code)]\n'+'\n'.join(fixed)+'\nfn main() {}\n')
   check(tc,'hints_suggested')
   recon['via_diagnostics']={'count':len(actual),'cases':actual,'all_verbatim_suggestions_compile':True}
   p=check(tc,'format_errors',101); errors=messages(p)
   assert len(errors)==2,errors
   format_actual=[]; fixed=[]
   for i,c in enumerate(CASES['format']):
    matching=[m for m in errors if any(span.get('is_primary') and span.get('line_start')==i+2 for span in m['spans'])]
    assert len(matching)==1,(c,matching)
    m=matching[0]
    assert ('`r#type` is neither a placement' if c['site']=='body' else '`r#type` is not a format') in m['message'],m['message']
    paths=re.findall(r'a format type is written as a path \(`([^`]+)`\)',m['message'])
    assert paths==['self::r#type'],m['message']
    old='POST p(r#type b: String);' if c['site']=='body' else 'GET p() -> r#type String;'
    new=old.replace('r#type',paths[0])
    fixed.append(c['source'].replace(old,new))
    format_actual.append({'site':c['site'],'suggested_path':paths[0],'diagnostic':m['message']})
   (PROBE/'src/bin/format_suggested.rs').write_text('#![allow(non_camel_case_types, dead_code)]\n'+'\n'.join(fixed)+'\nfn main() {}\n')
   check(tc,'format_suggested');check(tc,'script_suggested')
   recon['lowercase_format_errors']={'count':2,'cases':format_actual,'suggested_paths_compile':True}
   recon['script_printed_path_compiles_in_body_and_output']=True
   reconciliation[tc]=recon
  except Exception as e:
   failures.append({'toolchain':tc,'error':str(e),'traceback':traceback.format_exc()})
   print('FAIL '+tc+': '+str(e),flush=True)
  save('reconciliation.json',reconciliation);save('failures.json',failures)
except Exception as e:
 failures.append({'phase':'setup','error':str(e),'traceback':traceback.format_exc()});save('failures.json',failures)
 print(traceback.format_exc(),flush=True)
print(json.dumps({'failures':len(failures),'toolchains_completed':list(reconciliation),'commands':len(records)},indent=2),flush=True)
raise SystemExit(1 if failures else 0)
