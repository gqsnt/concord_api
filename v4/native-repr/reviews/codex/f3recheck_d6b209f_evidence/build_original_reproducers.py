from pathlib import Path
import subprocess,json
root=Path.cwd();p=root/'target/probes/migration';(p/'src').mkdir(parents=True,exist_ok=True)
(p/'Cargo.toml').write_text(f'''[package]
name="migration-rereview"
version="0.0.0"
edition="2024"
[dependencies]
concord={{path="{root}/crates/concord",default-features=false}}
[workspace]
''')
variants={
 'qualified':('codec::Text',''),
 'scoped_import':('Text','use crate::codec::Text;\nmod unrelated { #[allow(unused_imports)] use concord::Text; }\n'),
 'disabled_import':('Text','use crate::codec::Text;\n#[cfg(any())] use concord::Text;\n'),
 'commented_import':('Text','use crate::codec::Text;\n// use concord::Text;\n'),
}
for name,(codec,imports) in variants.items():
 text=imports+'use crate::codec;\nconcord::api! {\n pub client Migrated {\n  base "http://127.0.0.1";\n  POST p(body b: String via '+codec+');\n }\n}\n'
 (p/f'{name}-original.rs').write_text(text)
 (p/f'{name}.rs').write_text(text)
 subprocess.run(['python3','scripts/migrate_via.py',str(p/f'{name}.rs')],check=True)
lib='''#![allow(dead_code,unused_imports)]
pub mod codec {
 pub struct Text;
 impl concord::Encode<String> for Text {
  const CONTENT_TYPE:Option<&'static str>=Some("application/x-custom");
  fn encode(v:String)->Result<concord::reqwest::Body,concord::BodyError> {Ok(format!("CUSTOM:{v}").into())}
 }
}
concord::api! {
 pub client Expected { base "http://127.0.0.1"; POST p(codec::Text b: String); }
}
'''
for name in variants: lib+=f'mod {name} {{ include!("../{name}.rs"); }}\n'
lib+='''#[cfg(test)] mod tests {
 use super::*;
 fn http()->concord::reqwest::Client { concord::reqwest::Client::builder().no_proxy().build().unwrap() }
'''
for name in variants:
 lib+=f''' #[test] fn {name}() {{
  let h=http(); let expected=Expected::new(h.clone()).p("value").prepare().unwrap();
  let migrated={name}::Migrated::new(h).p("value").prepare().unwrap();
  let old=expected.request().body().unwrap().as_bytes(); let new=migrated.request().body().unwrap().as_bytes();
  println!("{name}: expected {{old:?}}; migrated {{new:?}}");
  assert_eq!(old,Some(b"CUSTOM:value".as_slice())); assert_eq!(new,old);
 }}\n'''
lib+='}\n';(p/'src/lib.rs').write_text(lib)
