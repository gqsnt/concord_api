//! Stable `C3-MIGRATE-*` diagnostics, captured identities and report output.

use serde_json::{Value, json};

/// Diagnostic severity. Only `Error` marks an unautomatable site.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }
}

macro_rules! codes {
    ($($variant:ident => $code:literal, $severity:ident, $automated:literal, $summary:literal;)*) => {
        /// Stable migration diagnostic codes.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub(crate) enum Code {
            $($variant,)*
        }

        impl Code {
            pub(crate) const ALL: &'static [Code] = &[$(Code::$variant,)*];

            pub(crate) const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)*
                }
            }

            pub(crate) const fn severity(self) -> Severity {
                match self {
                    $(Self::$variant => Severity::$severity,)*
                }
            }

            /// Whether the tool performed the corresponding source change.
            pub(crate) const fn automated(self) -> bool {
                match self {
                    $(Self::$variant => $automated,)*
                }
            }

            pub(crate) const fn summary(self) -> &'static str {
                match self {
                    $(Self::$variant => $summary,)*
                }
            }
        }
    };
}

codes! {
    ParseRust => "C3-MIGRATE-PARSE-RUST", Error, false,
        "the file is not valid Rust and was not examined";
    ParseDeclaration => "C3-MIGRATE-PARSE-DECLARATION", Error, false,
        "an `api!` invocation is not a valid v2.1 declaration";
    AlreadyV3 => "C3-MIGRATE-ALREADY-V3", Note, false,
        "an `api!` invocation already uses v3-only syntax and was left unchanged";
    UnresolvedReference => "C3-MIGRATE-UNRESOLVED-REFERENCE", Error, false,
        "an owner-qualified reference does not resolve in the v2.1 owner graph";
    ReservedOwnerName => "C3-MIGRATE-RESERVED-OWNER-NAME", Error, false,
        "a scope uses a name reserved by v3";
    PathAdapterBraces => "C3-MIGRATE-PATH-ADAPTER-BRACES", Error, false,
        "a path adapter type cannot be written inside a template hole";
    PathAdaptedLiteral => "C3-MIGRATE-PATH-ADAPTED-LITERAL", Error, false,
        "a path atom adapts a literal or concatenation, which a template cannot express";
    PathLiteral => "C3-MIGRATE-PATH-LITERAL", Error, false,
        "a literal path segment cannot be expressed as template text";
    KeyReplace => "C3-MIGRATE-KEY-REPLACE", Note, true,
        "a plain assignment over an earlier owner became exact `replace`";
    SensitivitySticky => "C3-MIGRATE-SENSITIVITY-STICKY", Warning, false,
        "a key that was sensitive earlier on the owner chain stays redacted in v3 after a non-sensitive override";
    RootDefaultTiming => "C3-MIGRATE-ROOT-DEFAULT-TIMING", Warning, false,
        "a root `#[default]` value's `Default::default()` now runs once when the builder is created, not lazily in `build()` or again after `reset_*`";
    WhenPresentWithoutFallback => "C3-MIGRATE-WHEN-PRESENT-NO-FALLBACK", Error, false,
        "`when_present` has no earlier owner, which v3 rejects";
    RemoveWithoutOwner => "C3-MIGRATE-REMOVE-NO-OWNER", Error, false,
        "`remove` targets a key that was never owned, which v3 rejects";
    RateWindowDuplicate => "C3-MIGRATE-RATE-WINDOW-DUPLICATE", Error, false,
        "a rate policy repeats a window duration, which v3 rejects";
    RateUnknownPolicy => "C3-MIGRATE-RATE-UNKNOWN-POLICY", Error, false,
        "a named rate attachment references an undefined policy";
    RateKeyEncoder => "C3-MIGRATE-RATE-KEY-ENCODER", Error, false,
        "a rate dimension adapter must become a v3 `RateKeyEncoder` with `ENCODING_ID`";
    PaginationInit => "C3-MIGRATE-PAGINATION-INIT", Note, true,
        "built-in pagination fields became a public Init value and a whole-cursor mapping";
    PaginationCustomController => "C3-MIGRATE-PAGINATION-CUSTOM-CONTROLLER", Error, false,
        "a custom pagination controller needs a public Init type and Cursor mapping";
    PaginationField => "C3-MIGRATE-PAGINATION-FIELD", Error, false,
        "a built-in pagination binding cannot be mapped to its v3 Init/Cursor contract";
    IdPin => "C3-MIGRATE-ID-PIN", Note, true,
        "the v2.1 endpoint ID was pinned explicitly";
    IdInvalid => "C3-MIGRATE-ID-INVALID", Error, false,
        "the endpoint needs an explicit ID but its v2.1 ID is not a valid v3 ID";
    SetterReplace => "C3-MIGRATE-SETTER-REPLACE", Note, true,
        "an optional-value `set_x` call became exact `replace_x`";
    SetterUnresolved => "C3-MIGRATE-SETTER-UNRESOLVED", Error, false,
        "a `set_x` call may target a Concord-optional value but its receiver is unresolved";
    SetterAmbiguous => "C3-MIGRATE-SETTER-AMBIGUOUS", Error, false,
        "a `set_x` call resolves to both optional and required values";
    PaginateCollectAll => "C3-MIGRATE-PAGINATE-COLLECT-ALL", Note, true,
        "`.paginate(PageLimit::CompleteWithin(n))` became `.collect_all(n)`";
    PaginatePrefix => "C3-MIGRATE-PAGINATE-PREFIX", Warning, true,
        "`.paginate(PageLimit::Prefix(n))` became `.collect_prefix(n)`; its output type changes";
    PaginateDynamic => "C3-MIGRATE-PAGINATE-DYNAMIC", Error, false,
        "`.paginate(..)` receives a runtime `PageLimit`; choose an explicit v3 terminal";
    PaginateUnresolved => "C3-MIGRATE-PAGINATE-UNRESOLVED", Error, false,
        "a `.paginate(..)` call is not proved to be the generated traversal of Concord's `PageLimit`; it was left unchanged";
    PageLimitImport => "C3-MIGRATE-PAGELIMIT-IMPORT", Note, true,
        "an unused `PageLimit` import was removed";
    PageLimitReference => "C3-MIGRATE-PAGELIMIT-REFERENCE", Error, false,
        "`PageLimit` no longer exists in v3 and is still referenced";
    RuntimeNonZero => "C3-MIGRATE-RUNTIME-NONZERO", Note, true,
        "a runtime configuration NonZero argument now passes its primitive with `.get()`";
    RuntimeNonZeroUnresolved => "C3-MIGRATE-RUNTIME-NONZERO-UNRESOLVED", Error, false,
        "a possible runtime configuration setter has an unresolved receiver";
    AcceptConst => "C3-MIGRATE-ACCEPT-CONST", Error, false,
        "a response representation `fn accept()` must become `const ACCEPT`";
    StoreOutcome => "C3-MIGRATE-STORE-OUTCOME", Error, false,
        "a `store_as_*` caller must handle `StoreOutcome::Superseded`";
    RequestRepresentation => "C3-MIGRATE-REQUEST-REPRESENTATION", Error, false,
        "a request representation must move exact media facts into `PreparedBody`";
    RateKeyPart => "C3-MIGRATE-RATE-KEY-PART", Error, false,
        "a `RateKeyPart` implementation must name its encoding with `ENCODING_ID`";
    RateLimiterAtomic => "C3-MIGRATE-RATE-LIMITER-ATOMIC", Error, false,
        "a custom `RateLimiter` must admit complete plans atomically and describe its enforcement";
    ProviderOrigins => "C3-MIGRATE-PROVIDER-ORIGINS", Warning, false,
        "a custom `CredentialProvider` using `ProviderHttp` must declare `http_origins`";
    PaginationControllerImpl => "C3-MIGRATE-PAGINATION-CONTROLLER-IMPL", Error, false,
        "a custom `PaginationController` needs `Init`, `Cursor`, `start` and `cursor`";
    PageResponseRepresentation => "C3-MIGRATE-PAGE-RESPONSE-REPRESENTATION", Error, false,
        "`PageResponseRepresentation` was removed in favor of Buffered driver plus controller bounds";
    PreludeImport => "C3-MIGRATE-PRELUDE-IMPORT", Note, true,
        "a declaration type that v2.1 resolved through the facade prelude is now imported explicitly";
    PreludeImportUnresolved => "C3-MIGRATE-PRELUDE-IMPORT-UNRESOLVED", Error, false,
        "a declaration type needs a facade prelude import that the tool cannot place";
    GeneratedShadow => "C3-MIGRATE-GENERATED-SHADOW", Error, false,
        "a declaration path would re-resolve through a v3 generated name or a relative `self`/`super` prefix";
    GeneratedNameShadow => "C3-MIGRATE-GENERATED-NAME-SHADOW", Error, false,
        "a call may reach a method that v3 generates and v2.1 did not, which would take precedence over the application method it called in v2.1";
    GeneratedMemberRemoved => "C3-MIGRATE-GENERATED-MEMBER-REMOVED", Error, false,
        "a call reached a generated method that v3 no longer generates; unchanged, it fails to compile or silently calls an application trait method of the same name";
    GeneratedNameCollision => "C3-MIGRATE-GENERATED-NAME-COLLISION", Error, false,
        "a declaration's values or children produce colliding v3 generated names, which v3 rejects";
    DeadValue => "C3-MIGRATE-DEAD-VALUE", Error, false,
        "a value has no semantic use, which v3 rejects";
    PaginatePrefixItems => "C3-MIGRATE-PAGINATE-PREFIX-ITEMS", Note, true,
        "an awaited `.paginate(PageLimit::Prefix(n))` became `.collect_prefix(n)` with `.into_items()`";
    PaginatedRequestType => "C3-MIGRATE-PAGINATED-REQUEST-TYPE", Note, true,
        "a `PaginatedRequest<_>` annotation became the selected v3 terminal type";
    PaginatedRequestReference => "C3-MIGRATE-PAGINATED-REQUEST-REFERENCE", Error, false,
        "`PaginatedRequest` no longer exists in v3 and is still referenced";
    BuilderPath => "C3-MIGRATE-BUILDER-PATH", Note, true,
        "a reference to the generated `ClientBuilder` now names it in the generated support module (`client::ClientBuilder`)";
    BuilderPathUnresolved => "C3-MIGRATE-BUILDER-PATH-UNRESOLVED", Error, false,
        "a reference that may name the generated `ClientBuilder` could not be moved to the generated support module";
    ApiMacroUnresolved => "C3-MIGRATE-API-MACRO-UNRESOLVED", Error, false,
        "an `api!` invocation whose macro path is not proved to be Concord's `api!` was not migrated";
    DependencyIdentity => "C3-MIGRATE-DEPENDENCY-IDENTITY", Error, false,
        "the package manifest does not establish that the Concord crate names denote the genuine Concord packages; the file was not migrated";
    NestingLimit => "C3-MIGRATE-NESTING-LIMIT", Error, false,
        "the file nests delimiters deeper than the tool's bound and was not examined";
    Internal => "C3-MIGRATE-INTERNAL", Error, false,
        "the migration tool could not apply a change consistently";
}

/// A diagnostic located by byte offset inside one file.
#[derive(Clone, Debug)]
pub(crate) struct RawDiagnostic {
    pub(crate) code: Code,
    pub(crate) offset: usize,
    pub(crate) message: String,
    pub(crate) suggestion: Option<String>,
}

impl RawDiagnostic {
    pub(crate) fn new(code: Code, offset: usize, message: impl Into<String>) -> Self {
        Self {
            code,
            offset,
            message: message.into(),
            suggestion: None,
        }
    }

    pub(crate) fn suggest(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// A diagnostic with a resolved file location.
#[derive(Clone, Debug)]
pub(crate) struct Diagnostic {
    pub(crate) code: Code,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) message: String,
    pub(crate) suggestion: Option<String>,
}

impl Diagnostic {
    pub(crate) fn severity(&self) -> Severity {
        self.code.severity()
    }

    pub(crate) fn render(&self) -> String {
        let mut text = format!(
            "{}:{}:{}: {}[{}]: {}",
            self.file,
            self.line,
            self.column,
            self.severity().label(),
            self.code.as_str(),
            self.message
        );
        if let Some(suggestion) = &self.suggestion {
            text.push_str("\n    suggested v3 spelling: ");
            text.push_str(&suggestion.replace('\n', "\n        "));
        }
        text
    }

    fn to_json(&self) -> Value {
        json!({
            "code": self.code.as_str(),
            "severity": self.severity().label(),
            "automated": self.code.automated(),
            "file": self.file,
            "line": self.line,
            "column": self.column,
            "message": self.message,
            "suggestion": self.suggestion,
        })
    }
}

/// One effective rate policy captured from the v2.1 source before conversion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PolicyIdentity {
    pub(crate) id: String,
    pub(crate) cost: u32,
    /// `(capacity, duration in milliseconds)` in source order.
    pub(crate) windows: Vec<(u64, u128)>,
    pub(crate) labels: Vec<String>,
    pub(crate) attachment: String,
}

/// The operational identity of one endpoint captured before conversion.
#[derive(Clone, Debug)]
pub(crate) struct EndpointIdentity {
    pub(crate) file: String,
    pub(crate) display_path: String,
    pub(crate) method: String,
    pub(crate) v2_id: String,
    pub(crate) v3_id: String,
    pub(crate) explicit: bool,
    pub(crate) pinned: bool,
    pub(crate) policies: Vec<PolicyIdentity>,
}

impl EndpointIdentity {
    fn to_json(&self) -> Value {
        json!({
            "file": self.file,
            "endpoint": self.display_path,
            "method": self.method,
            "v2_1_id": self.v2_id,
            "v3_id": self.v3_id,
            "explicit_in_v2_1": self.explicit,
            "pinned_by_migration": self.pinned,
            "rate_policies": self.policies.iter().map(|policy| json!({
                "id": policy.id,
                "cost": policy.cost,
                "windows": policy.windows.iter().map(|(capacity, millis)| json!({
                    "capacity": capacity,
                    "duration_ms": u64::try_from(*millis).unwrap_or(u64::MAX),
                })).collect::<Vec<_>>(),
                "key_labels": policy.labels,
                "attachment": policy.attachment,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Summary counts for one migration run.
#[derive(Clone, Debug, Default)]
pub(crate) struct Summary {
    pub(crate) files_scanned: usize,
    pub(crate) files_changed: usize,
    pub(crate) declarations: usize,
    pub(crate) edits: usize,
    pub(crate) errors: usize,
    pub(crate) warnings: usize,
    pub(crate) notes: usize,
}

impl Summary {
    pub(crate) fn render(&self) -> String {
        format!(
            "migrate-v3: scanned {} Rust file(s), {} v2.1 declaration(s); {} file(s) with {} automated edit(s); {} unautomatable site(s), {} warning(s), {} note(s)",
            self.files_scanned,
            self.declarations,
            self.files_changed,
            self.edits,
            self.errors,
            self.warnings,
            self.notes
        )
    }
}

pub(crate) fn report_json(
    input: &str,
    mode: &str,
    summary: &Summary,
    changed_files: &[(String, usize)],
    diagnostics: &[Diagnostic],
    identities: &[EndpointIdentity],
) -> Value {
    json!({
        "tool": "concord-xtask migrate-v3",
        "format_version": 1,
        "input": input,
        "mode": mode,
        "summary": {
            "files_scanned": summary.files_scanned,
            "declarations": summary.declarations,
            "files_changed": summary.files_changed,
            "automated_edits": summary.edits,
            "unautomatable": summary.errors,
            "warnings": summary.warnings,
            "notes": summary.notes,
        },
        "changed_files": changed_files.iter().map(|(file, edits)| json!({
            "file": file,
            "edits": edits,
        })).collect::<Vec<_>>(),
        "diagnostics": diagnostics.iter().map(Diagnostic::to_json).collect::<Vec<_>>(),
        "captured_identities": identities.iter().map(EndpointIdentity::to_json).collect::<Vec<_>>(),
        "codes": Code::ALL.iter().map(|code| json!({
            "code": code.as_str(),
            "severity": code.severity().label(),
            "automated": code.automated(),
            "summary": code.summary(),
        })).collect::<Vec<_>>(),
    })
}
