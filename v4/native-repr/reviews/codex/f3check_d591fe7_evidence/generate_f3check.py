"""Generate independent migration probes; never edit tracked sources."""
from pathlib import Path
import json
import shutil
import subprocess

ROOT = Path.cwd()
BASE = ROOT / 'target/probes'
P = BASE / 'matrix'
(P / 'src').mkdir(parents=True, exist_ok=True)
for sub in ['original', 'migrated', 'controls']:
    (P / sub).mkdir(exist_ok=True)

# Run the exact four archived F3/F3a reproducers against the current migration CLI.
shutil.copyfile(ROOT / 'v4/native-repr/reviews/codex/rereview_1174d5d_evidence/build_migration.py', BASE / 'build_original_reproducers.py')
with (BASE / 'original-generation.txt').open('w') as out:
    subprocess.run(['python3', str(BASE / 'build_original_reproducers.py')], check=True, stdout=out, stderr=subprocess.STDOUT)

manifest = '''[package]
name = "f3check-matrix"
version = "0.0.0"
edition = "2024"
[dependencies]
concord = { path = "ROOT/crates/concord", default-features = false, features = ["json"] }
tokio = { version = "=1.54.1", features = ["macros", "rt", "time"] }
[workspace]
'''.replace('ROOT', str(ROOT))
(P / 'Cargo.toml').write_text(manifest)

# A thread owns each one-request listener. finish() and Drop both join it, including on panic.
support = r'''
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
pub struct Observation<T> {
    pub body: Vec<u8>,
    pub content_types: Vec<String>,
    pub decoded: Result<T, String>,
}

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<Result<(Vec<u8>, Vec<String>), String>>>,
}
impl Server {
    fn start(reply: &[u8]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let reply = reply.to_vec();
        let task = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                if flag.load(Ordering::SeqCst) || Instant::now() > deadline {
                    return Err("listener stopped or timed out".into());
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(2)),
                    Err(e) => return Err(e.to_string()),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
            stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            let mut chunk = [0; 4096];
            let head_end = loop {
                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") { break i; }
                let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
                if n == 0 { return Err("EOF before request headers".into()); }
                buf.extend_from_slice(&chunk[..n]);
            };
            let head = String::from_utf8(buf[..head_end].to_vec()).map_err(|e| e.to_string())?;
            let headers: Vec<_> = head.lines().skip(1).filter_map(|l| l.split_once(':'))
                .map(|(n,v)| (n.to_ascii_lowercase(), v.trim().to_owned())).collect();
            let len: usize = headers.iter().find(|(n,_)| n == "content-length")
                .ok_or("missing content-length")?.1.parse().map_err(|_| "bad content-length")?;
            while buf.len() < head_end + 4 + len {
                let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
                if n == 0 { return Err("EOF before request body".into()); }
                buf.extend_from_slice(&chunk[..n]);
            }
            let body = buf[head_end+4..head_end+4+len].to_vec();
            let cts = headers.into_iter().filter(|(n,_)| n == "content-type").map(|(_,v)|v).collect();
            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reply.len());
            stream.write_all(response.as_bytes()).map_err(|e| e.to_string())?;
            stream.write_all(&reply).map_err(|e| e.to_string())?;
            stream.flush().map_err(|e| e.to_string())?;
            Ok((body, cts))
        });
        Self {url, stop, task: Some(task)}
    }
    fn finish(&mut self) -> (Vec<u8>, Vec<String>) {
        self.stop.store(true, Ordering::SeqCst);
        self.task.take().unwrap().join().expect("server thread panicked").expect("server I/O failed")
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(task) = self.task.take() { let _ = task.join(); }
    }
}
pub fn http() -> crate::c4::reqwest::Client {
    crate::c4::reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap()
}
pub async fn exchange<T>(make: impl FnOnce(String) -> crate::c4::Prepared<T>, reply: &[u8]) -> Observation<T> {
    let mut server = Server::start(reply);
    let result = make(server.url.clone()).send().await.map_err(|e| format!("{e:?}"));
    let (body, content_types) = server.finish();
    Observation {body, content_types, decoded: result}
}
'''
(P / 'src/wire.rs').write_text(support)

lib = r'''#![allow(dead_code, unused_imports, non_camel_case_types)]
extern crate concord as c4;
mod wire;
pub mod codec {
    macro_rules! custom {
        ($name:ident) => {
            pub struct $name;
            impl crate::c4::Encode<String> for $name {
                const CONTENT_TYPE: Option<&'static str> = Some("application/x-custom");
                fn encode(v: String) -> Result<crate::c4::reqwest::Body, crate::c4::BodyError> { Ok(format!("CUSTOM:{v}").into()) }
            }
            impl crate::c4::Decode<String> for $name {
                fn decode(v: crate::c4::bytes::Bytes) -> Result<String, crate::c4::BodyError> { Ok(format!("CUSTOM:{}", String::from_utf8(v.to_vec()).unwrap())) }
            }
            impl crate::c4::Encode<Vec<u8>> for $name {
                const CONTENT_TYPE: Option<&'static str> = Some("application/x-custom");
                fn encode(v: Vec<u8>) -> Result<crate::c4::reqwest::Body, crate::c4::BodyError> { Ok([b"CUSTOM:".as_slice(), &v].concat().into()) }
            }
            impl crate::c4::Decode<Vec<u8>> for $name {
                fn decode(v: crate::c4::bytes::Bytes) -> Result<Vec<u8>, crate::c4::BodyError> { Ok([b"CUSTOM:".as_slice(), v.as_ref()].concat()) }
            }
        }
    }
    custom!(Text); custom!(Json); custom!(Binary); custom!(Form);
}
'''
cases = []
def case(name, path, setup='', binary=False, json_reply=False, depth=False, macro='crate::c4::api', control_path=None, semantic_change=False):
    ty = 'Vec<u8>' if binary else 'String'
    decl = f'''{macro}! {{
    pub client Migrated {{
        base "http://127.0.0.1";
        POST p(body b: {ty} via {path}) -> {ty} via {path};
        POST optional(body b?: {ty} via {path});
    }}
}}
'''
    original = setup + '\n' + decl
    expected_path = control_path or path
    control = decl.replace('client Migrated', 'client Expected').replace(f'body b: {ty} via {path}', f'{expected_path} b: {ty}').replace(f'body b?: {ty} via {path}', f'{expected_path} b?: {ty}').replace(f'-> {ty} via {path}', f'-> {expected_path} {ty}')
    if depth:
        original = 'pub mod deeper {\n' + original + '}\n'
    (P / f'original/{name}.rs').write_text(original)
    (P / f'migrated/{name}.rs').write_text(original)
    # The control is inserted in the same Rust scope as the migrated declaration.
    (P / f'controls/{name}.rs').write_text(control)
    cases.append(dict(name=name, path=path, binary=binary, json_reply=json_reply, depth=depth, semantic_change=semantic_change))

case('f3_qualified', 'codec::Text', 'use crate::codec;')
case('f3a_scoped_import', 'Text', 'use crate::codec::Text; mod unrelated { use concord::Text; }')
case('f3a_cfg_disabled', 'Text', 'use crate::codec::Text; #[cfg(any())] use concord::Text;')
case('f3a_line_comment', 'Text', 'use crate::codec::Text; // use concord::Text;')
case('f3a_block_comment', 'Text', 'use crate::codec::Text; /* use concord::{Json, Text}; */')
case('cfg_selected_custom', 'Text', '#[cfg(all())] use crate::codec::Text; #[cfg(any())] use concord::Text;')
case('glob_custom', 'Text', 'use crate::codec::*; mod unrelated { use concord::*; }')
case('aliased_import', 'CustomText', 'use crate::codec::Text as CustomText; use concord::Text;')
case('self_path', 'self::Text', 'use crate::codec::Text;')
case('super_path', 'super::codec::Text')
case('crate_path', 'crate::codec::Text')
case('nested_module', 'Text', 'use crate::codec::Text; mod unrelated { use concord::Text; }', depth=True)
case('nested_super', 'super::Text', 'pub type Text = crate::codec::Text;', depth=False)
# super::Text above refers to a root alias (defined below).
case('downstream_form', 'Form', 'use crate::codec::Form;')
for marker, binary, jr in [('Text', False, False), ('Json', False, True), ('Binary', True, False)]:
    case(f'bare_builtin_{marker.lower()}', marker, f'use concord::{marker};', binary=binary, json_reply=jr)
    case(f'qualified_builtin_{marker.lower()}', f'concord::{marker}', binary=binary, json_reply=jr)
    case(f'absolute_builtin_{marker.lower()}', f'::concord::{marker}', binary=binary, json_reply=jr, control_path=f'crate::c4::{marker}')
    setup = f'mod concord {{ pub use crate::codec::{marker}; }}'
    case(f'local_concord_{marker.lower()}', f'concord::{marker}', setup, binary=binary, json_reply=jr, semantic_change=True)
    case(f'absolute_under_shadow_{marker.lower()}', f'::concord::{marker}', setup, binary=binary, json_reply=jr, control_path=f'crate::c4::{marker}')
case('alias_named_concord', 'concord::Text', 'use crate::codec as concord;', semantic_change=True)
case('macro_reexport_safe', 'facade::Text', 'mod facade { pub use crate::c4::{api, Text}; }', macro='facade::api')
case('macro_reexport_custom', 'facade::Text', 'mod facade { pub use crate::c4::api; pub use crate::codec::Text; }', macro='facade::api')
case('macro_reexport_shadow', 'concord::Json', 'mod concord { pub use crate::c4::api; pub use crate::codec::Json; }', macro='concord::api', json_reply=True, semantic_change=True)
case('nested_shadow_path', 'nested::concord::Json', 'mod nested { pub mod concord { pub use crate::codec::Json; } }')
case('bare_lowercase_text', 'text', 'type text = crate::codec::Text;', control_path='self::text', semantic_change=True)
case('bare_lowercase_json', 'json', 'type json = crate::codec::Json;', control_path='self::json', json_reply=True, semantic_change=True)
case('bare_lowercase_bytes', 'bytes', 'type bytes = crate::codec::Binary;', control_path='self::bytes', binary=True, semantic_change=True)
case('qualified_lowercase', 'self::text', 'type text = crate::codec::Text;')

with (BASE / 'matrix-generation.txt').open('w') as out:
    subprocess.run(['python3', 'scripts/migrate_via.py', *map(str, sorted((P/'migrated').glob('*.rs')))], check=True, stdout=out, stderr=subprocess.STDOUT)

lib += '\npub type Text = codec::Text;\n'
for c in cases:
    name = c['name']
    # Original and migrated scope are identical. Put the explicit format control beside it.
    migrated = (P / f'migrated/{name}.rs').read_text()
    control = (P / f'controls/{name}.rs').read_text()
    if c['depth']:
        migrated = migrated.rsplit('}', 1)[0] + control + '}\n'
    else:
        migrated += control
    (P / f'src/{name}.rs').write_text(migrated)
    lib += f'pub mod {name};\n'

lib += '\n#[cfg(test)] mod tests {\nuse super::*;\n'
for c in cases:
    n = c['name']; module = n + ('::deeper' if c['depth'] else '')
    value = 'b"value".to_vec()' if c['binary'] else '"value".to_owned()'
    reply = 'br#""reply""#' if c['json_reply'] else 'b"reply"'
    lib += f'''#[tokio::test]
async fn {n}() {{
    let expected = wire::exchange(|url| {module}::Expected::new(wire::http()).with_base_url(url).unwrap().p({value}).prepare().unwrap(), {reply}).await;
    let migrated = wire::exchange(|url| {module}::Migrated::new(wire::http()).with_base_url(url).unwrap().p({value}).prepare().unwrap(), {reply}).await;
    println!("{n}: expected {{expected:?}}; migrated {{migrated:?}}");
    assert!(expected.decoded.is_ok(), "control failed: {{expected:?}}");
    assert!(migrated.decoded.is_ok(), "migrated failed: {{migrated:?}}");
    let absent = {module}::Migrated::new(wire::http()).optional().prepare().unwrap();
    assert!(absent.request().body().is_none());
    assert!(absent.request().headers().get("content-type").is_none());
    assert_eq!(migrated, expected, "migration changed the selected codec");
}}
'''
lib += '}\n'
(P / 'src/lib.rs').write_text(lib)
(P / 'cases.json').write_text(json.dumps(cases, indent=2)+'\n')

# A real Cargo rename, with no `concord` extern-prelude entry.
R = BASE/'renamed'; (R/'src').mkdir(parents=True, exist_ok=True)
(R/'Cargo.toml').write_text(manifest.replace('f3check-matrix', 'f3check-renamed').replace('concord = {', 'c4 = { package = "concord",'))
(R/'src/wire.rs').write_text(support)
renamed = '''use c4 as c4;
mod wire;
mod facade { pub use c4::{api, Json}; }
c4::api! {
    pub client Renamed { base "http://127.0.0.1";
        POST p(body b: String via c4::Json) -> String via c4::Json;
        POST absolute(body b: String via c4::Text) -> String via ::c4::Text;
    }
}
facade::api! { pub client Reexport { base "http://127.0.0.1";
    POST p(body b: String via facade::Json) -> String via facade::Json;
} }
#[tokio::test] async fn renamed_and_reexported() {
    let a = wire::exchange(|u| Renamed::new(wire::http()).with_base_url(u).unwrap().p("value").prepare().unwrap(), br#""reply""#).await;
    let b = wire::exchange(|u| Reexport::new(wire::http()).with_base_url(u).unwrap().p("value").prepare().unwrap(), br#""reply""#).await;
    println!("renamed: {a:?}; reexported: {b:?}");
    assert_eq!(a, b);
    assert_eq!(a.body, br#""value""#);
    assert_eq!(a.content_types, ["application/json"]);
    assert_eq!(a.decoded, Ok("reply".to_owned()));
    let c = wire::exchange(|u| Renamed::new(wire::http()).with_base_url(u).unwrap().absolute("value").prepare().unwrap(), b"reply").await;
    assert_eq!(c.body, b"value"); assert_eq!(c.decoded, Ok("reply".to_owned()));
}
'''
(R/'original.rs').write_text(renamed)
(R/'src/lib.rs').write_text(renamed)
with (BASE/'renamed-generation.txt').open('w') as out:
    subprocess.run(['python3','scripts/migrate_via.py',str(R/'src/lib.rs')],check=True,stdout=out,stderr=subprocess.STDOUT)
(R/'src/bin').mkdir(exist_ok=True)
for label, original in {
    'absolute_body': 'c4::api! { pub client A { base "http://127.0.0.1"; POST p(body b: String via ::c4::Text); } }\nfn main() {}\n',
    'absolute_output': 'c4::api! { pub client A { base "http://127.0.0.1"; GET p() -> String via ::c4::Text; } }\nfn main() {}\n',
    'absolute_custom_body': 'extern crate c4 as codec;\nc4::api! { pub client A { base "http://127.0.0.1"; POST p(body b: String via ::codec::Text); } }\nfn main() {}\n',
    'absolute_alias_output': 'extern crate c4 as codec;\nc4::api! { pub client A { base "http://127.0.0.1"; GET p() -> String via ::codec::Text; } }\nfn main() {}\n',
}.items():
    (R/f'{label}-original.rs').write_text(original)
    output=R/f'src/bin/{label}.rs'
    output.write_text(original)
    with (BASE/f'{label}-generation.txt').open('w') as out:
        subprocess.run(['python3','scripts/migrate_via.py',str(output)],check=True,stdout=out,stderr=subprocess.STDOUT)
print(f'Generated original 4-case reproducer, {len(cases)} paired wire cases, and Cargo-rename probe.')
