"""Check rustdoc search or editor hover and completion for a generated client."""
import argparse
import json
import os
import re
import shutil
import subprocess
import threading
import time
import tomllib
from html import unescape
from pathlib import Path
from queue import Queue

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "target/call-documentation"
DOC = ROOT / "target/doc"
ENV = {
    **os.environ,
    "CARGO_NET_OFFLINE": "true",
    "CARGO_BUILD_JOBS": "4",
    "CARGO_TARGET_DIR": str(ROOT / "target"),
    "RAYON_NUM_THREADS": "4",
}
API = """\
pub type Player = String;
pub type Count = u32;
pub type Problem = String;

pub mod names {
    pub type E = String;
    pub type T = u32;
    pub type Call = u16;
    pub type Endpoint = u8;
    pub type Value = String;
    pub type Data = bool;
    macro_rules! problem_type { () => { E }; }

    concord::api! {
        pub client Named {
            base "https://names.test";
            read concord::Json;
            problem problem_type!();
            GET inspect(e: E, t: T, call: Call, endpoint: Endpoint, value: Value, data: Data) {
                query { e; t; call; endpoint; value; data; }
            }
            GET list(cursor?: u32) -> Vec<u32> {
                query { cursor; }
                paginate concord::by_offset(cursor);
            }
        }
    }
}

concord::api! {
    pub client Riot {
        base "https://riot.test";
        read concord::Json;
        problem Problem;
        scope champion_mastery {
            GET top(puuid: Player, count?: Count) { query {puuid; count;} }
            GET history(page?: u32) -> Vec<u32> {
                query { page; }
                paginate concord::by_offset(page);
            }
            POST submit();
            POST export(page?: u32) -> Vec<u32> {
                query { page; }
                paginate concord::by_offset(page);
            }
        }
    }
}
"""


def problem_client(name, problem, visibility=""):
    return f"""\
    concord::api! {{
        {visibility}client {name} {{
            base "https://types.test";
            read concord::Json;
            problem {problem};
            GET get();
            GET list(cursor?: u32) -> Vec<u32> {{
                query {{ cursor; }}
                paginate concord::by_offset(cursor);
            }}
        }}
    }}
"""


def hover_probe(name, setup, call, paged, parameters=""):
    return f"""\
pub fn {name}({parameters}) {{
{setup}
    let call = {call};
    let paged = {paged};
    let _ = call.clone().with_retry(concord::Retry::none());
    let _ = call.clone().with_timeout(std::time::Duration::ZERO);
    let _ = call.clone().with_idle_timeout(std::time::Duration::ZERO);
    let _ = call.clone().edit_request(|_| {{}});
    let _ = call.clone().request();
    let _ = call.clone().into_request();
    let _ = call.clone().read_response(concord::reply(200, ""));
    let _ = call.clone().send();
    let _ = call.clone().send_full();
    let _ = call.clone().send_raw();
    let _ = call.send_if_none_match("tag");
    let _ = paged.pages();
}}
"""


API += 'pub mod direct_name {\n    pub type E = String;\n' + problem_client("Named", "E", "pub ") + '}\n'
USER = """\
use call_docs_api::{Riot, riot::champion_mastery::Top};

pub fn probe(riot: &Riot) {
    let call = riot.champion_mastery().top("p");
    let _ = Top::count(call.clone(), 3);
    let _ = call.clone().send();
    let _ = call.clone().count(2);
    let _ = call.clone().count(3);

    let history = riot.champion_mastery().history();
    let submit = riot.champion_mastery().submit();
    let export = riot.champion_mastery().export();
    let _ = call.request();
    let _ = history.request();
    let _ = submit.request();
    let _ = export.request();
}
"""
USER += hover_probe("module_problem", "", 'riot.champion_mastery().top("p")',
                    "riot.champion_mastery().history()", "riot: &Riot")
USER += hover_probe("block_problem", """\
    #[allow(non_camel_case_types)]
    type retry = String;
    #[allow(non_camel_case_types)]
    type etag = u32;
""" + problem_client("Local", "(retry, etag)") + "    let client = Local::new();",
                    "client.get()", "client.list()")
USER += hover_probe("module_named_e", "    let client = call_docs_api::direct_name::Named::new();",
                    "client.get()", "client.list()")
USER += hover_probe("module_problem_macro", "    let client = call_docs_api::names::Named::new();",
                    'client.inspect("e", 2_u32, 3_u16, 4_u8, "v", true)', "client.list()")
USER += hover_probe("block_named_e", "    type E = String;\n" + problem_client("Local", "E")
                    + "    let client = Local::new();", "client.get()", "client.list()")
USER += hover_probe("block_problem_macro", """\
    type E = String;
    macro_rules! problem_type { () => { E }; }
""" + problem_client("Local", "problem_type!()") + "    let client = Local::new();",
                    "client.get()", "client.list()")

PROBLEMS = {
    "module_problem": "Problem",
    "block_problem": "(retry,etag)",
    "module_named_e": "__DeclaredProblem",
    "module_problem_macro": "__DeclaredProblem",
    "block_named_e": "__DeclaredProblem",
    "block_problem_macro": "__DeclaredProblem",
}
ELIGIBILITY = {
    "call": {"send_if_none_match"},
    "history": {"pages", "send_if_none_match"},
    "submit": set(),
    "export": {"pages"},
}
CALL_METHODS = (
    "with_retry", "with_timeout", "with_idle_timeout", "edit_request", "request",
    "into_request", "read_response", "send", "send_full", "send_raw", "send_if_none_match", "pages",
)
PROBLEM_METHODS = {"with_retry", "send", "send_full", "send_raw", "send_if_none_match", "pages"}
CODE_HEADERS = re.compile(
    r'<(h[34]|pre) class="(?:code-header|rust item-decl)"[^>]*>(.*?)</\1>', re.S
)
PRIVATE_NAME = re.compile(r"(?<!\w)__\w+")
SELF_PROJECTION = re.compile(r"<\s*Self\s+as\b")


def cargo(*args, cwd=OUT):
    return subprocess.check_output(["cargo", *args, "--offline"], cwd=cwd, env=ENV)


def write_project():
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "Cargo.toml").write_text('[workspace]\nresolver = "2"\nmembers = ["api", "caller"]\n')
    for name, source in [("api", API), ("caller", USER)]:
        crate = OUT / name
        crate.mkdir(exist_ok=True)
        package = f'[package]\nname = "call_docs_{name}"\nversion = "0.0.0"\nedition = "2021"\n'
        library = '[lib]\npath = "lib.rs"\n'
        dependency = f'concord = {{ package = "concord-api", path = {json.dumps(str(ROOT / "concord-api"))} }}\n'
        if name == "caller":
            dependency += 'call_docs_api = { path = "../api" }\n'
        (crate / "Cargo.toml").write_text(package + library + "[dependencies]\n" + dependency)
        (crate / "lib.rs").write_text(source)


def editor_project():
    sysroot = subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()
    sysroot_src = Path(sysroot) / "lib/rustlib/src/rust/library"
    if not sysroot_src.is_dir():
        raise SystemExit(f"call-editor requires rust-src at {sysroot_src}")
    lines = cargo("check", "--workspace", "-j", "4", "--message-format=json").splitlines()
    messages = [json.loads(line) for line in lines]
    metadata = json.loads(cargo("metadata", "--format-version", "1"))
    build = {m["package_id"]: m for m in messages if m["reason"] == "build-script-executed"}
    macros = {
        m["package_id"]: next(file for file in m["filenames"] if file.endswith((".so", ".dylib", ".dll")))
        for m in messages if m["reason"] == "compiler-artifact" and "proc-macro" in m["target"]["kind"]
    }
    cfg = subprocess.check_output(["rustc", "--print", "cfg"], text=True).splitlines()
    packages = {}
    crates = []
    for package in metadata["packages"]:
        target = next((t for t in package["targets"] if set(t["kind"]) & {"lib", "proc-macro"}), None)
        if target is None:
            continue
        package_id = package["id"]
        packages[package_id] = len(crates)
        manifest = str(Path(package["manifest_path"]).parent)
        env = {"CARGO_MANIFEST_DIR": manifest, "CARGO_PKG_NAME": package["name"], "CARGO_PKG_VERSION": package["version"]}
        extra = build.get(package_id, {})
        if extra:
            env.update(extra["env"])
            env["OUT_DIR"] = extra["out_dir"]
        crate = {
            "display_name": target["name"], "root_module": target["src_path"],
            "edition": target["edition"], "deps": [], "cfg": cfg + extra.get("cfgs", []),
            "env": env, "is_workspace_member": package_id in metadata["workspace_members"],
            "is_proc_macro": package_id in macros,
        }
        if package_id in macros:
            crate["proc_macro_dylib_path"] = macros[package_id]
        crates.append(crate)
    for node in metadata["resolve"]["nodes"]:
        if node["id"] not in packages:
            continue
        crate = crates[packages[node["id"]]]
        crate["cfg"] += [f'feature="{name}"' for name in node["features"]]
        crate["deps"] = [
            {"crate": packages[dep["pkg"]], "name": dep["name"]}
            for dep in node["deps"] if dep["pkg"] in packages
            and any(kind["kind"] is None for kind in dep["dep_kinds"])
        ]
    project = OUT / "rust-project.json"
    project.write_text(json.dumps({
        "sysroot": sysroot, "sysroot_src": str(sysroot_src),
        "crates": crates,
    }))
    return project


def editor(project):
    events = Queue()
    with (OUT / "editor.stderr").open("w") as errors, subprocess.Popen(
        ["rust-analyzer"], cwd=OUT, env=ENV,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors,
    ) as process:
        def read():
            while True:
                headers = {}
                while True:
                    line = process.stdout.readline()
                    if not line:
                        events.put(None)
                        return
                    if line == b"\r\n":
                        break
                    key, value = line.decode().split(":", 1)
                    headers[key.lower()] = value.strip()
                events.put(json.loads(process.stdout.read(int(headers["content-length"]))))

        threading.Thread(target=read, daemon=True).start()
        sequence = 0

        def send(**message):
            data = json.dumps({"jsonrpc": "2.0", **message}).encode()
            process.stdin.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
            process.stdin.flush()

        def receive():
            result = events.get(timeout=40)
            assert result is not None, "rust-analyzer stopped before completing the probe"
            if "method" in result and "id" in result:
                config = result.get("params", {}).get("items", [])
                send(id=result["id"], result=[{} for _ in config] if result["method"] == "workspace/configuration" else None)
            return result

        def request(method, params):
            nonlocal sequence
            sequence += 1
            send(id=sequence, method=method, params=params)
            while True:
                message = receive()
                if message.get("id") == sequence and "method" not in message:
                    assert "error" not in message, message
                    return message["result"]

        try:
            request("initialize", {
                "processId": os.getpid(), "rootUri": OUT.as_uri(),
                "capabilities": {"experimental": {"serverStatusNotification": True}},
                "initializationOptions": {
                    "linkedProjects": [str(project)], "checkOnSave": False,
                    "cargo": {"buildScripts": {"enable": False}},
                    "cachePriming": {"enable": False}, "numThreads": 4,
                    "procMacro": {"enable": True, "processes": 1},
                },
            })
            send(method="initialized", params={})
            source = USER.replace("call.clone().count(3);", "call.cou;")
            uri = (OUT / "caller/lib.rs").as_uri()
            send(method="textDocument/didOpen", params={
                "textDocument": {"uri": uri, "languageId": "rust", "version": 1, "text": source},
            })
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                message = receive()
                if message.get("method") == "experimental/serverStatus" and message["params"].get("quiescent"):
                    assert message["params"]["health"] == "ok", message
                    break
            else:
                raise AssertionError("rust-analyzer did not finish loading")

            def at(token, offset=0, start=0):
                prefix = source[:source.index(token, start) + offset]
                return {"textDocument": {"uri": uri}, "position": {
                    "line": prefix.count("\n"), "character": len(prefix.rsplit("\n", 1)[-1]),
                }}

            results = {
                "completion": request("textDocument/completion", at("call.cou;", len("call.cou"))),
                "factory": request("textDocument/hover", at('top("p")', 1)),
                "setter": request("textDocument/hover", at("count(call", 1)),
                "send": request("textDocument/hover", at("send()", 1)),
                "fluent": request("textDocument/hover", at("count(2)", 1)),
                "eligibility": {
                    call: request("textDocument/completion", at(f"{call}.request()", len(call) + 1))
                    for call in ELIGIBILITY
                },
                "call_methods": {
                    case: {
                        method: request("textDocument/hover", at(
                            f".{method}(", 1, source.index(f"pub fn {case}("),
                        ))
                        for method in CALL_METHODS
                    }
                    for case in PROBLEMS
                },
            }
            (OUT / "editor.json").write_text(json.dumps(results, indent=2))
            request("shutdown", None)
            send(method="exit")
            return results
        finally:
            process.terminate()


def display(html):
    return unescape(re.sub(r"<[^>]+>", "", html))


def rustdoc_signature(page, method):
    section = page.split(f'id="method.{method}"', 1)[1]
    header = section.split("<h4", 1)[1].split("</h4>", 1)[0].split(">", 1)[1]
    return display(header)


def call_page_violations(path, problem=None):
    page = path.read_text()
    signatures = [display(content) for _, content in CODE_HEADERS.findall(page)]
    retry = rustdoc_signature(page, "with_retry")
    assert retry in signatures and any("fn send(" in sig for sig in signatures), path
    problems = [
        f"{path.parent.name}: {signature}"
        for signature in signatures
        if PRIVATE_NAME.search(signature) or SELF_PROJECTION.search(signature)
    ]
    if problem:
        for method in ("with_retry", "send", "send_if_none_match"):
            signature = rustdoc_signature(page, method)
            if not re.search(rf"\b{problem}\b", signature):
                problems.append(f"{path.parent.name}: missing problem type {problem} in {signature}")
    return problems


def hover_signature(result, method, include_impl=False):
    contents = (result or {}).get("contents", {})
    text = contents.get("value", "") if isinstance(contents, dict) else ""
    prefix = f"pub fn {method}("
    before, found, tail = text.partition(prefix)
    if not found:
        return ""
    display = (before if include_impl else "") + found + tail.split("\n\n", 1)[0]
    return "".join(display.split())


CONVERSIONS = re.compile(r"Text and list conversions: (<a\b[^>]*><code>[^<]*</code></a>|.{0,120})", re.S)
CONVERSION_LINK = re.compile(r'^<a href="https://docs\.rs/concord-api/([^/"]+)/([^"#]+)"><code>concord::arg::Arg</code></a>$')
LINKS = ROOT / "target/call-documentation/links"


def check_links():
    """Document tour-gen with --no-deps in a folder of its own, as an author's crate is, and check
    that every conversion link names this release and a page of the facade's documentation."""
    shutil.rmtree(LINKS / "doc", ignore_errors=True)
    env = {**ENV, "CARGO_TARGET_DIR": str(LINKS)}
    for package in ("tour-gen", "concord-api"):
        subprocess.check_output(["cargo", "doc", "--offline", "-j", "4", "--no-deps", "-p", package],
                                cwd=ROOT, env=env)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    links, wrong = 0, []
    for page in sorted((LINKS / "doc/tour_gen").rglob("*.html")):
        for link in CONVERSIONS.findall(page.read_text()):
            links += 1
            found = CONVERSION_LINK.match(link)
            if not found:
                wrong.append(f"{page.relative_to(LINKS / 'doc')}: not a link to docs.rs: {link}")
            elif found[1] != version:
                wrong.append(f"{page.relative_to(LINKS / 'doc')}: version {found[1]}, not {version}")
            elif not (LINKS / "doc" / found[2]).is_file():
                wrong.append(f"{page.relative_to(LINKS / 'doc')}: no page {found[2]} in the facade's documentation")
    if wrong:
        raise SystemExit("Conversion links that do not reach the facade's page:\n" + "\n".join(sorted(set(wrong))))
    if not links:
        raise SystemExit("No setter of tour-gen documents its text and list conversions")
    print(f"All {links} conversion links name docs.rs for {version} and a page of the facade.")


def check_search():
    cargo("doc", "-j", "4", "-p", "call_docs_api", "--no-deps")
    subprocess.run(
        ["node", str(Path(__file__).with_name("rustdoc_search.cjs")), str(DOC)],
        check=True,
    )
    call_page = DOC / "call_docs_api/riot/struct.Call.html"
    assert 'id="method.count"' not in call_page.read_text(), "The Call page exposes a specific setter"
    for file, method, spelling in [
        ("struct.ChampionMastery.html", "top", "Arg<Player>"),
        ("champion_mastery/enum.Top.html", "count", "Option<Count>"),
    ]:
        signature = rustdoc_signature((DOC / "call_docs_api/riot" / file).read_text(), method)
        compact = "".join(signature.split())
        assert spelling in compact and "__Types" not in compact, (
            f"Rustdoc for {method} must show {spelling}: {signature}"
        )
        if method == "count":
            assert re.search(r"pubfncount\((?:mut)?__call:", compact), (
                f"Rustdoc must name __call first: {signature}"
            )
    print("Rustdoc search finds Top::count and its signature shows the authored type.")
    check_links()
    cargo("doc", "-j", "4", "-p", "tour-gen", "--no-deps", cwd=ROOT)
    violations = []
    for client, problem in [("tasks", None), ("feed", None), ("files", None),
                            ("riot", "RiotProblem"), ("shapes", "ShapesProblem")]:
        violations.extend(call_page_violations(DOC / "tour_gen" / client / "struct.Call.html", problem))
    named = DOC / "call_docs_api/names/named"
    violations.extend(call_page_violations(named / "struct.Call.html", "E"))
    violations.extend(call_page_violations(
        DOC / "call_docs_api/direct_name/named/struct.Call.html", "E"
    ))
    for page in sorted(named.rglob("*.html")):
        if page.name in {"type.Error.html", "type.Retry.html", "struct.Call.html"}:
            continue
        for _, content in CODE_HEADERS.findall(page.read_text()):
            signature = re.sub(r"\b__call(?=\s*:)", "", display(content))
            if PRIVATE_NAME.search(signature):
                violations.append(f"{page.name}: {signature}")
    if violations:
        raise SystemExit("\n".join(violations))
    print("Call pages show public parameter names and the declared problem type.")


def labels(completion):
    items = completion.get("items", []) if isinstance(completion, dict) else completion or []
    return {item["label"].split("(", 1)[0] for item in items}


def check_editor():
    results = editor(editor_project())
    errors = []
    if "count" not in labels(results["completion"]):
        errors.append("count is absent from completion in the dependent crate")
    for call, expected in ELIGIBILITY.items():
        offered = labels(results["eligibility"][call])
        if "request" not in offered:
            errors.append(f"{call}: no shared request method in completion")
        if (actual := offered & {"pages", "send_if_none_match"}) != expected:
            errors.append(f"{call}: expected {sorted(expected)}, got {sorted(actual)}")
    if not errors:
        print("Editor completes count, pages and send_if_none_match where available.", flush=True)
    for kind, method, spelling in [
        ("factory", "top", "Arg<Player>"), ("setter", "count", "Option<Count>"),
        ("fluent", "count", "Option<Count>"), ("send", "send", ",Problem>"),
    ]:
        signature = hover_signature(results[kind], method)
        if spelling not in signature or any(
            name in signature for name in ("__Input", "__Types", "__DeclaredProblem")
        ):
            errors.append(f"{kind} hover must show {spelling}: {signature}")
    setter_hover = hover_signature(results["setter"], "count")
    if not re.search(r"^pubfncount\((?:mut)?__call:", setter_hover):
        errors.append(f"setter hover must name __call first: {setter_hover}")
    for case, methods in results["call_methods"].items():
        problem = PROBLEMS[case]
        for method, result in methods.items():
            signature = hover_signature(result, method, include_impl=True)
            aliases = {problem} if problem.startswith("__") and method in PROBLEM_METHODS else set()
            names = set(PRIVATE_NAME.findall(signature))
            if not signature or names != aliases or "<Selfas" in signature:
                errors.append(f"{case} {method}: {signature or 'no signature'}")
            elif method in PROBLEM_METHODS:
                spelling = f"Retry<{problem}>" if method == "with_retry" else f",{problem}>"
                if spelling not in signature:
                    errors.append(f"{case} {method} must show {spelling}: {signature}")
    if errors:
        raise SystemExit("\n".join(errors))
    print("Editor hovers show authored types and the setter's first parameter.")
    print("The problem alias appears only for E and macro-spelled types, in modules and functions.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", choices=["call-doc-search", "call-editor"])
    probe = parser.parse_args().probe
    tools = ("node", "cargo", "rustdoc") if probe == "call-doc-search" else ("rust-analyzer", "cargo", "rustc")
    for tool in tools:
        if not shutil.which(tool):
            raise SystemExit(f"{probe} requires {tool} on PATH")
    write_project()
    if probe == "call-doc-search":
        check_search()
    else:
        check_editor()


if __name__ == "__main__":
    main()
