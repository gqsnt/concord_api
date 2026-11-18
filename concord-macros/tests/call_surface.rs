//! Compiler and documentation checks for call types and methods.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{fs, io};

const CLIENT: &str = include_str!("support/call_client.rs");

struct Compiler {
    directory: PathBuf,
    dependencies: PathBuf,
    concord: PathBuf,
}

impl Compiler {
    fn new(suite: &str) -> io::Result<Self> {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/call-probes")
            .join(format!("{}-{suite}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let mut dependencies = std::env::current_exe()?;
        assert!(dependencies.pop(), "the test executable has no parent");
        let mut libraries = Vec::new();
        for entry in fs::read_dir(&dependencies)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("libconcord-") && name.ends_with(".rlib"))
            {
                libraries.push((entry.metadata()?.modified()?, entry.path()));
            }
        }
        let (_, concord) = libraries
            .into_iter()
            .max_by_key(|(modified, _)| *modified)
            .ok_or_else(|| {
                io::Error::other(format!(
                    "concord was not built in {}",
                    dependencies.display()
                ))
            })?;
        let compiler = Self {
            directory,
            dependencies,
            concord,
        };
        let library = compiler.compile("client", CLIENT, false)?;
        assert!(library.status.success(), "{}", stderr(&library));
        Ok(compiler)
    }

    fn compile(&self, name: &str, source: &str, dependent: bool) -> io::Result<Output> {
        self.compile_with(name, source, dependent, &[])
    }

    fn compile_with(
        &self,
        name: &str,
        source: &str,
        dependent: bool,
        extra: &[&str],
    ) -> io::Result<Output> {
        let path = self.directory.join(format!("{name}.rs"));
        fs::write(&path, source)?;
        let mut rustc = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        rustc
            .args(["--edition=2021", "--crate-type=rlib", "--crate-name", name])
            .arg("--color=never")
            .args(["-Adead_code", "-Aunused_imports"])
            .arg("-L")
            .arg(format!("dependency={}", self.dependencies.display()))
            .arg("--extern")
            .arg(format!("concord={}", self.concord.display()))
            .arg("--out-dir")
            .arg(&self.directory);
        if dependent {
            rustc.arg("--extern").arg(format!(
                "client={}",
                self.directory.join("libclient.rlib").display()
            ));
        }
        rustc.args(extra).arg(path).output()
    }

    fn document(&self, name: &str, source: &str, cfg: &[&str]) -> io::Result<PathBuf> {
        let path = self.directory.join(format!("{name}.rs"));
        fs::write(&path, source)?;
        let output = self.directory.join(format!("{name}-doc"));
        let rustdoc = Command::new("rustdoc")
            .args(["--edition=2021", "--crate-name", name])
            .args(cfg)
            .arg("-L")
            .arg(format!("dependency={}", self.dependencies.display()))
            .arg("--extern")
            .arg(format!("concord={}", self.concord.display()))
            .arg("-o")
            .arg(&output)
            .arg(path)
            .output()?;
        assert!(rustdoc.status.success(), "{}", stderr(&rustdoc));
        Ok(output.join(name))
    }

    fn reject_in_both_contexts(
        &self,
        name: &str,
        code: &str,
        error: &str,
        evidence: &str,
    ) -> io::Result<()> {
        assert!(!code.contains("__"), "{name} uses a reserved internal name");
        for (context, description, dependent) in [
            ("invocation", CLIENT, false),
            ("dependent", "use client::{gate, Gate};", true),
        ] {
            let output = self.compile(
                &format!("{name}_{context}"),
                &format!("{description}\n{code}"),
                dependent,
            )?;
            let text = stderr(&output);
            assert!(
                !output.status.success()
                    && text.contains(error)
                    && public_rule_names(&text).contains(evidence),
                "{name} ({context}) should fail with {error} and {evidence}\n{text}"
            );
        }
        Ok(())
    }
}

fn public_rule_names(text: &str) -> String {
    text.replace("concord_core::call::", "")
        .replace("concord::call::", "")
        .replace("client::gate::", "")
        .replace("gate::", "")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn c1_11_only_a_factory_returns_a_call() -> io::Result<()> {
    let compiler = Compiler::new("endpoint")?;
    compiler.reject_in_both_contexts(
        "construct_endpoint_type",
        "fn probe() { let _ = gate::Plain; }",
        "error[E0423]",
        "enum named `Plain`",
    )?;
    compiler.reject_in_both_contexts(
        "endpoint_type_as_parameter",
        "fn old(_: gate::Plain) {} fn probe() { old(Gate::new().plain(1)); }",
        "error[E0308]",
        "Call<Plain>",
    )?;
    Ok(())
}

#[test]
fn c1_11_c8_6_c8_8_c10_10_only_the_description_grants_call_capabilities() -> io::Result<()> {
    let compiler = Compiler::new("capabilities")?;
    let probes = [
        (
            "await_paged",
            "async fn probe() { let _ = Gate::new().paged().await; }",
            "error[E0277]",
            "remove the `.await`",
        ),
        (
            "page_unpaged",
            "fn probe() { let _ = Gate::new().plain(1).pages(); }",
            "error[E0599]",
            "NoPaginate: DeclaresPaginate",
        ),
        (
            "conditional_post",
            r#"fn probe() { let _ = Gate::new().post().send_if_none_match("\"v1\""); }"#,
            "error[E0599]",
            "Post: IsGetOrHead",
        ),
        (
            "conditional_other",
            r#"fn probe() { let _ = Gate::new().delete().send_if_none_match("tag"); }"#,
            "error[E0599]",
            "NotGetOrHead: IsGetOrHead",
        ),
        (
            "conditional_prepare_post",
            r#"fn probe() { let _ = Gate::new().post().prepare_if_none_match("\"v1\""); }"#,
            "error[E0599]",
            "prepare_if_none_match",
        ),
        (
            "conditional_preparation_through_page_trait",
            "fn probe<C: concord::page::Call>(call: C) { let _ = call.prepare().unwrap().conditional(); } fn use_post() { probe(Gate::new().post()); }",
            "error[E0599]",
            "prepare",
        ),
        (
            "grant_conditional_mode",
            "impl concord::call::IsGetOrHead for <gate::Post as gate::Endpoint>::Conditional {}",
            "error[E0117]",
            "not defined in the current crate",
        ),
        (
            "read_input",
            "fn probe() { let _ = Gate::new().plain(1).id; }",
            "error[E0609]",
            "field `id`",
        ),
        (
            "set_input",
            "fn probe() { let mut call = Gate::new().paged(); call.only_plain = Some(2); }",
            "error[E0609]",
            "field `only_plain`",
        ),
        (
            "foreign_setter",
            "fn probe() { let _ = Gate::new().paged().only_plain(2); }",
            "error[E0599]",
            "only_plain",
        ),
        (
            "cross_slot_read",
            "fn probe(slot: impl concord::page::Slot<gate::Call<gate::Plain>, Value = u32>) { let _ = slot.get(&Gate::new().paged()); }",
            "error[E0308]",
            "Call<Paged>",
        ),
        (
            "cross_slot_write",
            "fn probe(slot: impl concord::page::Slot<gate::Call<gate::Plain>, Value = u32>) { let _ = slot.set(Gate::new().paged(), 2); }",
            "error[E0308]",
            "Call<Paged>",
        ),
        (
            "register_endpoint",
            "struct Extra; impl gate::Endpoint for Extra { type Value = (); }",
            "error[E0277]",
            "Sealed",
        ),
    ];
    for (name, code, error, evidence) in probes {
        compiler.reject_in_both_contexts(name, code, error, evidence)?;
    }

    // The pager binds the actual generated slot. An opaque Slot bound in
    // caller code would hide a second implementation for another endpoint.
    for (name, attempt) in [
        (
            "concrete_slot_get",
            "let _ = concord::page::Slot::get(&cursor, &plain);",
        ),
        (
            "concrete_slot_set",
            "let _ = concord::page::Slot::set(&cursor, plain, 2);",
        ),
    ] {
        let pager =
            format!("move |call, _| {{ let plain = Gate::new().plain(1); {attempt} Some(call) }}");
        let source = CLIENT.replace("concord::by_offset(cursor)", &pager);
        assert!(!source.contains("__"));
        let output = compiler.compile(name, &source, false)?;
        let text = stderr(&output);
        assert!(
            !output.status.success()
                && text.contains("error[E0308]")
                && text.contains("Call<Paged>")
                && text.contains("Call<Plain>"),
            "{name} must reject use of the concrete slot on Plain
{text}"
        );
    }
    Ok(())
}

/// Awaiting a paged call: the message without the two parts of its `IntoFuture` hint that are
/// expected, the location inside the macro invocation and the note that the error originates
/// in the macro. The hint must be there, in either wording of the compiler ("is
/// conditionally implemented" from Rust 1.97, "is implemented" before). Its header and the
/// requirement it names stay in the text and are checked like the rest; the private names
/// are checked on the message as the compiler printed it.
fn without_impl_location(text: &str) -> String {
    let headers = [
        "help: the trait `IntoFuture` is conditionally implemented for",
        "help: the trait `IntoFuture` is implemented for",
    ];
    let mut kept = Vec::new();
    let mut lines = text.lines();
    let mut found = false;
    while let Some(line) = lines.next() {
        kept.push(line.to_owned());
        if !headers.iter().any(|header| line.starts_with(header)) {
            continue;
        }
        found = true;
        // The location: the `-->` line and the snippet of the invocation, up to the note.
        for line in lines.by_ref() {
            if line
                .trim_start()
                .starts_with("= note: this error originates in the macro")
            {
                break;
            }
            let snippet = line.trim_start().starts_with("-->")
                || line.trim_start().starts_with("...")
                || line
                    .trim_start()
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .trim_start()
                    .starts_with('|');
            if !snippet {
                kept.push(line.to_owned());
            } else if let Some((_, requirement)) =
                line.split_once("unsatisfied requirement introduced here: ")
            {
                kept.push(format!("requirement: {requirement}"));
            }
        }
    }
    assert!(found, "no hint of the IntoFuture impl:\n{text}");
    kept.join("\n")
}

#[test]
fn c1_18_rule_diagnostics_name_the_call_and_the_bound_at_the_call_site() -> io::Result<()> {
    let compiler = Compiler::new("rule_diagnostics")?;
    let mut violations = Vec::new();
    for (name, code, required) in [
        (
            "await_paged",
            "async fn probe() { let _ = Gate::new().paged().await; }",
            [
                "error[E0277]",
                "Call<Paged>",
                "is not a future",
                "remove the `.await`",
            ],
        ),
        (
            "page_unpaged",
            "fn probe() { let _ = Gate::new().plain(1).pages(); }",
            [
                "error[E0599]",
                "Call<Plain>",
                "NoPaginate: DeclaresPaginate",
                "pages",
            ],
        ),
        (
            "conditional_post",
            r#"fn probe() { let _ = Gate::new().post().send_if_none_match("tag"); }"#,
            [
                "error[E0599]",
                "Call<Post>",
                "Post: IsGetOrHead",
                "send_if_none_match",
            ],
        ),
        (
            "conditional_other",
            r#"fn probe() { let _ = Gate::new().delete().send_if_none_match("tag"); }"#,
            [
                "error[E0599]",
                "Call<Delete>",
                "NotGetOrHead: IsGetOrHead",
                "send_if_none_match",
            ],
        ),
    ] {
        for (context, declaration, dependent) in [
            ("invocation", CLIENT, false),
            ("dependent", "use client::{gate, Gate};", true),
        ] {
            let output = compiler.compile(
                &format!("{name}_{context}"),
                &format!("{declaration}\n{code}"),
                dependent,
            )?;
            let printed = stderr(&output);
            // No private name anywhere, the hint of a paged `.await` included.
            if printed.contains("__") {
                violations.push(format!(
                    "{name} ({context}) names a private item\n{printed}"
                ));
            }
            let text = if name == "await_paged" {
                // Awaiting a paged call names the generic `IntoFuture` impl and its unmet bound
                // in one hint, pointing into the description (the impl carries no
                // `#[diagnostic::do_not_recommend]`): only its location is set aside.
                without_impl_location(&printed)
            } else {
                printed
            };
            let expected_line = declaration.bytes().filter(|byte| *byte == b'\n').count() + 2;
            let primary = text
                .lines()
                .find(|line| line.trim_start().starts_with("-->"));
            let rules = public_rule_names(&text);
            let missing: Vec<_> = required
                .iter()
                .filter(|needle| !rules.contains(**needle))
                .collect();
            if output.status.success()
                || !missing.is_empty()
                || text.contains("__")
                || text.contains("originates in the macro")
                || text.lines().any(|line| line.contains("| / concord::api!"))
                || !primary.is_some_and(|line| line.contains(&format!(".rs:{expected_line}:")))
            {
                violations.push(format!("{name} ({context}), missing {missing:?}\n{text}"));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
    Ok(())
}

#[test]
fn c8_8_discarded_calls_give_the_text_for_each_operation() -> io::Result<()> {
    const GENERAL: &str = "a call does nothing until sent or awaited";
    const PAGED: &str = "await `.send()` for one page, or use `.pages()` for all pages";
    let compiler = Compiler::new("must_use")?;
    let mut failures = Vec::new();
    for (name, statement, expected) in [
        ("plain_factory", "Gate::new().plain(1);", GENERAL),
        ("paged_factory", "Gate::new().paged();", PAGED),
        ("post_factory", "Gate::new().post();", GENERAL),
        (
            "plain_setter",
            "Gate::new().plain(1).only_plain(2);",
            GENERAL,
        ),
        ("paged_setter", "Gate::new().paged().cursor(2);", PAGED),
        (
            "plain_function",
            "gate::Plain::only_plain(Gate::new().plain(1), 2);",
            GENERAL,
        ),
        (
            "paged_function",
            "gate::Paged::cursor(Gate::new().paged(), 2);",
            PAGED,
        ),
        (
            "plain_timeout",
            "Gate::new().plain(1).with_timeout(Duration::from_secs(1));",
            GENERAL,
        ),
        (
            "paged_timeout",
            "Gate::new().paged().with_timeout(Duration::from_secs(1));",
            GENERAL,
        ),
        ("clone_call", "Gate::new().plain(1).clone();", GENERAL),
    ] {
        let output = compiler.compile(
            name,
            &format!("#![deny(unused_must_use)]\nuse client::{{gate, Gate}};\nuse std::time::Duration;\nfn probe() {{ {statement} }}"),
            true,
        )?;
        let text = stderr(&output);
        let found = text.lines().any(|line| {
            let line = line.trim();
            line == format!("= note: {expected}") || line == format!("= help: {expected}")
        });
        if output.status.success()
            || !text.contains("that must be used")
            || !found
            || (expected == GENERAL && text.contains(PAGED))
        {
            failures.push(format!("{name}: expected {expected}\n{text}"));
        }
    }
    let awaited = compiler.compile(
        "awaited",
        "#![deny(unused_must_use)]\nuse client::Gate;\nasync fn probe() { Gate::new().paged().await; }",
        true,
    )?;
    let text = stderr(&awaited);
    if awaited.status.success()
        || !text.contains("is not a future")
        || text.contains(GENERAL)
        || text.contains(PAGED)
    {
        failures.push(format!(
            "awaited: expected the pager error without a must-use message\n{text}"
        ));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

const ATTRIBUTES: &str = r#"
#[cfg(feature = "extra")]
pub type FeatureInput = u32;

concord::api! {
    pub client Attributed {
        base "https://attributes.test";
        read concord::Json;
        GET visible(visible_option?: u32) { query { visible_option; } }

        #[deprecated(note = "use visible instead")]
        GET old(old_option?: u32) { query { old_option; } }

        #[doc(hidden)]
        GET secret_endpoint(secret_option?: u32) { query { secret_option; } }

        #[cfg(feature = "extra")]
        GET feature_endpoint(feature_option?: FeatureInput) -> Vec<u32> {
            query { feature_option; }
            paginate concord::by_offset(feature_option);
        }
    }
}
"#;

#[test]
fn c1_12_each_deprecated_entry_point_warns() -> io::Result<()> {
    let compiler = Compiler::new("deprecated")?;
    let mut failures = Vec::new();
    for (name, code, kind, member) in [
        ("factory", "let _ = Attributed::new().old();", "method", "old"),
        ("setter", "#[allow(deprecated)] let call = Attributed::new().old(); let _ = call.old_option(3);", "method", "old_option"),
        ("function", "#[allow(deprecated)] let call = Attributed::new().old(); let _ = attributed::Old::old_option(call, 3);", "associated function", "old_option"),
    ] {
        let output = compiler.compile(name, &format!("#![deny(deprecated)]\n{ATTRIBUTES}\nfn probe() {{ {code} }}"), false)?;
        let text = stderr(&output);
        let warning = text.lines().any(|line| {
            line.contains(&format!("use of deprecated {kind}"))
                && line.contains(&format!("{member}`: use visible instead"))
        });
        if output.status.success() || !warning {
            failures.push(format!("{name}: expected a deprecation of {kind} {member}\n{text}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

fn public_pages(path: &Path) -> io::Result<Vec<(PathBuf, String)>> {
    let mut pages = Vec::new();
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            pages.extend(public_pages(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "html") {
            let html = fs::read_to_string(&path)?;
            pages.push((path, html));
        }
    }
    Ok(pages)
}

#[test]
fn c1_12_hidden_endpoint_has_no_public_pages_or_signatures() -> io::Result<()> {
    let compiler = Compiler::new("hidden_endpoint")?;
    let page = compiler.document("hidden_endpoint", ATTRIBUTES, &[])?;
    assert!(page.join("attributed/enum.Visible.html").is_file());
    assert!(page.join("attributed/enum.Old.html").is_file());
    let mut leaks = Vec::new();
    for (path, html) in public_pages(&page)? {
        for name in ["SecretEndpoint", "secret_endpoint", "secret_option"] {
            if html.contains(name) {
                leaks.push(format!("{} exposes {name}", path.display()));
            }
        }
    }
    assert!(leaks.is_empty(), "{}", leaks.join("\n"));
    Ok(())
}

#[test]
fn c1_12_cfg_removes_the_endpoint_type_factory_and_setters() -> io::Result<()> {
    let compiler = Compiler::new("cfg_endpoint")?;
    let output = compiler.compile(
        "feature_off",
        &format!(
            "{ATTRIBUTES}\nfn probe() {{ let _ = Attributed::new().visible().visible_option(3); }}"
        ),
        false,
    )?;
    assert!(output.status.success(), "{}", stderr(&output));

    let output = compiler.compile(
        "missing_factory",
        &format!("{ATTRIBUTES}\nfn probe() {{ let _ = Attributed::new().feature_endpoint(); }}"),
        false,
    )?;
    let text = stderr(&output);
    assert!(
        !output.status.success()
            && text.contains("error[E0599]")
            && text.contains("feature_endpoint"),
        "{text}"
    );

    let output = compiler.compile(
        "missing_endpoint_type",
        &format!("{ATTRIBUTES}\nuse attributed::FeatureEndpoint;"),
        false,
    )?;
    let text = stderr(&output);
    assert!(
        !output.status.success()
            && text.contains("error[E0432]")
            && text.contains("FeatureEndpoint"),
        "{text}"
    );

    const ON: &[&str] = &["--cfg", "feature=\"extra\""];
    let output = compiler.compile_with("feature_on", &format!("{ATTRIBUTES}\nfn probe() {{ let call = Attributed::new().feature_endpoint().feature_option(3); let _ = attributed::FeatureEndpoint::feature_option(call, 2).pages(); }}"), false, ON)?;
    assert!(output.status.success(), "{}", stderr(&output));

    let off = compiler.document("cfg_off_docs", ATTRIBUTES, &[])?;
    let on = compiler.document("cfg_on_docs", ATTRIBUTES, ON)?;
    assert!(on.join("attributed/enum.FeatureEndpoint.html").is_file());
    assert!(fs::read_to_string(on.join("struct.Attributed.html"))
        .unwrap()
        .contains("id=\"method.feature_endpoint\""));
    let mut leaks = Vec::new();
    for (path, html) in public_pages(&off)? {
        for name in ["FeatureEndpoint", "feature_endpoint", "feature_option"] {
            if html.contains(name) {
                leaks.push(format!("{} exposes {name}", path.display()));
            }
        }
    }
    assert!(leaks.is_empty(), "{}", leaks.join("\n"));
    Ok(())
}

const RIOT_DOCS: &str = r#"
pub type Count = u32;

concord::api! {
    pub client Riot {
        base "https://riot.test";
        scope champion_mastery {
            GET top(count?: Count) { query { count; } }
        }
    }
}
"#;

#[test]
fn c10_4_the_endpoint_page_labels_the_call_type() -> io::Result<()> {
    let compiler = Compiler::new("doc_call_type")?;
    let docs = compiler.document("doc_call_type", RIOT_DOCS, &[])?;
    let page = fs::read_to_string(docs.join("riot/champion_mastery/enum.Top.html")).unwrap();
    let call = "<code>riot::Call&lt;riot::champion_mastery::Top&gt;</code>";
    assert!(
        page.contains(&format!("Call type: {call}")),
        "expected Call type on Top's page, found {:?}",
        page.split_once(call)
            .and_then(|(before, _)| before.rsplit_once("<p>"))
            .map(|(_, label)| label)
    );
    assert!(!page.contains(&format!("Value: {call}")));
    Ok(())
}

#[test]
fn c1_6_c10_4_rustdoc_setter_shows_the_call_parameter_and_authored_type() -> io::Result<()> {
    let compiler = Compiler::new("doc_setter_type")?;
    let docs = compiler.document("doc_setter_type", RIOT_DOCS, &[])?;
    let call = fs::read_to_string(docs.join("riot/struct.Call.html"))?;
    assert!(
        !call.contains("id=\"method.count\""),
        "the shared Call page shows an endpoint-specific setter"
    );
    let page = fs::read_to_string(docs.join("riot/champion_mastery/enum.Top.html")).unwrap();
    let (_, method) = page.split_once("id=\"method.count\"").unwrap();
    let (_, header) = method.split_once("<h4 class=\"code-header\">").unwrap();
    let (header, _) = header.split_once("</h4>").unwrap();
    let mut signature = String::new();
    let mut tag = false;
    for ch in header.chars() {
        match ch {
            '<' => tag = true,
            '>' => tag = false,
            _ if !tag && !ch.is_whitespace() => signature.push(ch),
            _ => {}
        }
    }
    let signature = signature
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("mut__call:", "__call:");
    assert!(
        signature.contains("fncount(__call:Call<Top>,count:implArg<Option<Count>>")
            && !signature.contains("__Types"),
        "unexpected rendered setter signature: {signature}"
    );
    Ok(())
}
