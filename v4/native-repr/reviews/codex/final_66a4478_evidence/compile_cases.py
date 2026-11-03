"""Independent compiler oracles. No production edits; all crates and logs live here."""
import json, os, pathlib, shutil, subprocess, sys
root=pathlib.Path(__file__).resolve().parent
work=root/'compile-work'
(work/'src').mkdir(parents=True,exist_ok=True)
(root/'compile-cases').mkdir(exist_ok=True)
manifest='''[package]
name="independent-compile"
version="0.0.0"
edition="2024"
[features]
default=["codecs"]
codecs=["wire/json", "wire/form"]
[dependencies]
wire={package="concord",path="../../../crates/concord",default-features=false}
[workspace]
'''
(work/'Cargo.toml').write_text(manifest)
(work/'src/lib.rs').write_text('')
env=dict(os.environ,CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(root.parent/'probe-build'))
toolchain=sys.argv[1] if len(sys.argv)>1 else ''
base=['cargo']+(['+'+toolchain] if toolchain else [])
subprocess.run(base+['generate-lockfile','--offline','--manifest-path',str(work/'Cargo.toml')],env=env,check=True,capture_output=True)
def api(inner,params='',defaults='',prefix=''):
 return prefix+'\nwire::api! { pub client Api'+('('+params+')' if params else '')+' { base "http://127.0.0.1"; '+defaults+' '+inner+' } }\n'
native_body="pub struct NativeBody; impl wire::Encode<wire::reqwest::Body> for NativeBody { const CONTENT_TYPE:Option<&'static str>=None; fn encode(v:wire::reqwest::Body)->Result<wire::reqwest::Body,wire::BodyError>{Ok(v)} }"
native='use wire::reqwest::header::HeaderValue as HV; type Alias=HV;'
cases=[
 ('missing_request',api('POST p(body p: String);'),False,'no `request` default',True),
 ('missing_response',api('GET p() -> String;'),False,'no `response` default',True),
 ('body_default',api('POST p(json p: String = String::new());'),False,'body cannot have a default',True),
 ('endpoint_statement',api('GET p() { response text; }'),False,'write the format',True),
 ('explicit_optional_ct',api('POST p(json("application/custom") b?: String);','header("content-type") ct?: HV',prefix=native),False,'keep one',True),
 ('composed_ct_remove_readd',api('POST p(json("application/custom") b: String) { headers { remove "content-type"; "content-type" = "readded"; } }','header("content-type") ct?: HV',prefix=native),False,'keep one',True),
 ('owner_media_ct',api('POST p(body b: String);','header("content-type") ct?: HV','request json("application/custom");',native),False,'keep one',True),
 ('remove_conflict',api('POST p(json("application/custom") b: String) { headers { remove "content-type"; } }','header("content-type") ct?: HV',prefix=native),True,'',True),
 ('local_discards_media',api('POST p(json b: String);','header("content-type") ct?: HV','request json("application/custom");',native),True,'',True),
 ('invalid_media',api('POST p(json("text/plain; broken") b: String);'),False,'media type',True),
 ('invalid_constant',api('POST p(Bad b: String);',prefix='pub struct Bad; impl wire::Encode<String> for Bad { const CONTENT_TYPE:Option<&\'static str>=Some("bad\\nheader"); fn encode(v:String)->Result<wire::reqwest::Body,wire::BodyError>{Ok(v.into())} }'),False,'error[E0080]',True),
 ('replaced_constant',api('POST p(Bad("text/plain") b: String);',prefix='pub struct Bad; impl wire::Encode<String> for Bad { const CONTENT_TYPE:Option<&\'static str>=Some("bad\\nheader"); fn encode(v:String)->Result<wire::reqwest::Body,wire::BodyError>{Ok(v.into())} }'),True,'',True),
 ('output_media',api('GET p() -> json("text/plain") String;'),False,'media type',True),
 ('removed_via',api('POST p(body p: String via wire::Text);'),False,'via',True),
 ('reserved_payload',api('GET p() -> json;'),False,'payload',True),
 ('lexical_vec',api('GET p() -> Vec<String>; GET q() -> Vec<String> {}','', 'response json;'),True,'',True),
 ('lexical_names',api('POST p(json: String, json query: String) { query { "q" = json; } } GET q(header("x") json?: String);'),True,'',True),
 ('raw_payload',api('GET p() -> r#json;',defaults='response text;',prefix='#[allow(non_camel_case_types)] type r#json=String;'),True,'',True),
 ('native_path',api('GET p(v: Alias) { path "{v}"; }',prefix=native),False,'written as text',True),
 ('native_query',api('GET p(query v: Alias);',prefix=native),False,'written as text',True),
 ('native_template',api('GET p(v: Alias) { headers { "x" = "{v}"; } }',prefix=native),False,'written as text',True),
 ('native_subdomain',api('GET p(v: Alias) { subdomain "{v}"; }',prefix=native),False,'written as text',True),
 ('native_each_query',api('GET p(query each v: Vec<Alias>);',prefix=native),False,'written as text',True),
 ('native_borrowed',api('GET p(header("x") v: Alias);',params='header("x-root") v: Alias = &V',prefix=native+' static V:HV=HV::from_static("v"); pub fn run(){let a=Api::new(wire::reqwest::Client::new());let _=a.p(&V).set_v(&V).prepare();}'),True,'',False),
 ('feature_off_in_place',api('POST p(json b: String) -> form String;'),False,'gated behind',False),
 ('feature_off_default',api('POST p(body b: String) -> String; POST q(body b: String) -> String;',defaults='request json; response form;'),False,'gated behind',False),
 ('unused_defaults',api('GET p();',defaults='request json; response form;'),True,'',False),
 ('disabled_consumers',api('#[cfg(any())] scope gone { POST p(body p: Unknown) -> Unknown; } GET p();',defaults='request json; response form;'),True,'',False),
 ('cfg_union_live',api('#[cfg(any())] POST p(body p: Unknown); #[cfg(all())] POST q(body p: String);',defaults='request json;'),False,'gated behind',False),
 ('nonclone_declaration',api('POST p(NativeBody b: wire::reqwest::Body);',prefix=native_body),True,'',False),
 ('nonclone_call',api('POST p(NativeBody b: wire::reqwest::Body);',prefix=native_body+'pub fn bad(a:Api){let _=a.p(wire::reqwest::Body::from("data")).clone();}'),False,'Clone',False),
]
results=[]
for name,source,passes,needle,features in cases:
 (root/'compile-cases'/f'{name}.rs').write_text(source)
 (work/'src/lib.rs').write_text(source)
 cmd=base+['check','--offline','--locked','--manifest-path',str(work/'Cargo.toml')]+([] if features else ['--no-default-features'])
 p=subprocess.run(cmd,env=env,text=True,capture_output=True)
 log=p.stdout+p.stderr
 (root/'compile-cases'/f'{name}.{toolchain or "stable"}.txt').write_text(log)
 ok=(p.returncode==0)==passes and (not needle or needle in log)
 print(('PASS' if ok else 'FAIL'),name,'exit',p.returncode,flush=True)
 if not ok: print(log,flush=True)
 results.append(dict(name=name,expected_pass=passes,returncode=p.returncode,oracle=needle,ok=ok))
(root/f'compile-results-{toolchain or "stable"}.json').write_text(json.dumps(results,indent=2)+'\n')
sys.exit(0 if all(r['ok'] for r in results) else 1)
