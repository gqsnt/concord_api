from pathlib import Path
import json
root=Path.cwd(); out=root/'target/probes'; crate=out/'focused'; (crate/'src/bin').mkdir(parents=True,exist_ok=True); (crate/'tests').mkdir(exist_ok=True)
(crate/'Cargo.toml').write_text(f'''[package]
name = "focused-rereview"
version = "0.0.0"
edition = "2024"
publish = false
[dependencies]
wire = {{ package = "concord", path = "{root}/crates/concord", default-features = false }}
tokio = {{ version = "1", features = ["macros", "rt", "time"] }}
[workspace]
''')
prefix='''#![allow(dead_code, unused_imports)]
use wire::{ToHeaderValue, reqwest::header::HeaderValue};
use std::{borrow::Cow, sync::Arc, rc::Rc};
#[derive(Clone)]
pub struct Dual(pub &'static str);
impl ToHeaderValue for Dual {
 fn to_header_value(&self)->Option<HeaderValue> { let mut h=HeaderValue::from_static(self.0); h.set_sensitive(true); Some(h) }
}
impl std::fmt::Display for Dual { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.write_str(self.0) } }
type Alias = Dual;
'''
cases={}
def add(name,decl,ok=False,diagnostic='placed both as a header value and as text'):
 cases[name]={'ok':ok,'diagnostic':None if ok else diagnostic}
 (crate/'src/bin'/f'{name}.rs').write_text(prefix+'\nwire::api! {\n'+decl+'\n}\nfn main() {}\n')
def endpoint(name,ty='Dual',arg='v',places='path "{v}"; query { "q" = v; } headers { "x-dual" = v; "x-copy" = "{v}"; }',ok=False):
 add(name,f'pub client Api {{ base "http://example.test"; GET go({arg}: {ty}) {{ {places} }} }}',ok)
endpoint('original')
for wrap in ['Box<Dual>','Arc<Dual>','Rc<Dual>',"Cow<'static, Dual>","&'static Dual",'Alias','Box<Arc<Alias>>']:
 endpoint('wrapper_'+str(len(cases)),wrap)
endpoint('optional',arg='v?',places='query { "q" = v; } headers { "x-dual" = v; "x-copy" = "{v}"; }')
endpoint('defaulted',arg='v',ty='Dual = Dual("S3CR3T")')
endpoint('option_each',ty='Option<Dual>',places='query { "q" = each v; } headers { "x-dual" = each v; }')
endpoint('vec_each',ty='Vec<Dual>',places='query { "q" = each v; } headers { "x-dual" = each v; }')
endpoint('wrapped_each',ty='Vec<Arc<Dual>>',places='query { "q" = each v; } headers { "x-dual" = each v; }')
endpoint('optional_each',ty='Vec<Dual>',arg='v?',places='query { "q" = each v; } headers { "x-dual" = each v; }')
endpoint('optional_type',ty='Option<Dual>',places='query { "q" = v; } headers { "x-dual" = v; }',ok=False)
cases['optional_type']['diagnostic']='written as text here and has no `Display`'
endpoint('direct_placement',arg='header("x-dual") v',places='path "{v}";')
endpoint('subdomain',places='subdomain "{v}"; headers { "x-dual" = v; }')
endpoint('template_only',places='headers { "x-dual" = v; "x-copy" = "{v}"; }')
add('owner_reference','''pub client Api(header("x-dual") v: Dual) { base "http://example.test"; GET go() { path "{client.v}"; query { "q" = client.v; } } }''')
add('scope_reference','''pub client Api { base "http://example.test"; scope child(header("x-dual") v: Dual) { GET go() { path "{child.v}"; } } }''')
add('separate_endpoints','''pub client Api(v: Dual) { base "http://example.test"; GET a() { headers { "x-dual" = client.v; } } GET b() { path "{client.v}"; } }''')
for op in ['override','append']:
 add(op,f'''pub client Api {{ base "http://example.test"; headers {{ "x-dual" = "base"; }} GET go(v: Dual) {{ path "{{v}}"; headers {{ {op} "x-dual" = v; }} }} }}''')
add('optional_override','''pub client Api { base "http://example.test"; headers { "x-dual" = "base"; } GET go(v?: Dual) { query { "q" = v; } headers { override "x-dual" = v else inherit; } } }''')
add('cfg_enabled','''pub client Api { base "http://example.test"; #[cfg(all())] GET go(v: Dual) { path "{v}"; headers { "x-dual" = v; } } }''')
add('cfg_disabled','''pub client Api { base "http://example.test"; #[cfg(any())] GET go(v: Dual) { path "{v}"; headers { "x-dual" = v; } } GET safe(); }''',True)
add('cfg_owner_text','''pub client Api(v: Dual) { base "http://example.test"; GET a() { headers { "x-dual" = client.v; } } #[cfg(any())] GET b() { path "{client.v}"; } }''')
endpoint('sensitive_marker',arg='#[sensitive] v',ok=True)
endpoint('sensitive_header_name',places='path "{v}"; query { "q" = v; } headers { "authorization" = v; "x-copy" = "{v}"; }',ok=True)
endpoint('sensitive_query_name',places='path "{v}"; query { "token" = v; } headers { "x-dual" = v; "x-copy" = "{v}"; }',ok=True)
endpoint('header_only',places='headers { "x-dual" = v; }',ok=True)
endpoint('string',ty='String',ok=True)
endpoint('integer',ty='u64',ok=True)
endpoint('boolean',ty='bool',ok=True)
for i,ty in enumerate(["&'static str",'Box<String>','Arc<String>',"Cow<'static,str>",'Rc<String>']): endpoint(f'ordinary_wrapper_{i}',ty=ty,ok=True)
endpoint('ordinary_each',ty='Vec<String>',places='query { "q" = each v; } headers { "x-dual" = each v; }',ok=True)
(crate/'compile-cases.json').write_text(json.dumps(cases,indent=2)+'\n')
(out/'run_compile.py').write_text('''from pathlib import Path
import subprocess,json,os,sys
root=Path.cwd(); crate=root/'target/probes/focused'; results=[]
for tool in ['stable','1.85']:
 for name,c in json.loads((crate/'compile-cases.json').read_text()).items():
  cmd=['cargo']+(['+1.85'] if tool=='1.85' else [])+['check','--locked','--offline','--manifest-path',str(crate/'Cargo.toml'),'--bin',name]
  p=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env={**os.environ,'CARGO_TARGET_DIR':str(root/'target/probe-build')})
  (crate/f'{name}.{tool}.txt').write_text(p.stdout)
  passed=(p.returncode==0)==c['ok'] and (c['ok'] or c['diagnostic'] in p.stdout)
  results.append({'name':name,'toolchain':tool,'expected_compile':c['ok'],'exit':p.returncode,'passed':passed})
  print(tool,name,'PASS' if passed else 'FAIL',flush=True)
(root/'target/probes/compile-results.json').write_text(json.dumps(results,indent=2)+'\\n')
sys.exit(not all(r['passed'] for r in results))
''')
print(f'wrote {len(cases)} compiler cases; renamed dependency wire')
