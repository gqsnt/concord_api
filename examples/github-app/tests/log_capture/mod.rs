//! Test-only raw `tracing` capture: a bare `Subscriber` (no formatter, filter or redaction step)
//! that records every field of every span, span update and event, of every target, before any
//! check. Shared by the J1 caller tests (`#[path]`), so all three use the same recorder.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Unsigned(u64),
    Other(String),
}

#[derive(Default)]
struct Fields(BTreeMap<String, Value>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .insert(field.name().into(), Value::Text(value.into()));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), Value::Unsigned(value));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0
            .insert(field.name().into(), Value::Other(format!("{value:?}")));
    }
}

/// One recorded span creation, span update or event.
#[derive(Clone, Debug)]
pub struct Seen {
    pub what: &'static str,
    pub target: String,
    pub name: String,
    pub level: String,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Default, Debug)]
struct State {
    seen: Vec<Seen>,
    /// Span id → target, to attribute updates.
    spans: BTreeMap<u64, String>,
}

#[derive(Clone, Default)]
pub struct Capture(Arc<Mutex<State>>);

struct Recorder {
    state: Arc<Mutex<State>>,
    next: AtomicU64,
    /// Which callsites this subscriber is interested in (its own filter, on metadata alone).
    select: fn(&Metadata<'_>) -> bool,
}

impl Recorder {
    fn push(&self, what: &'static str, meta: &Metadata<'_>, fields: Fields) {
        self.state.lock().unwrap().seen.push(Seen {
            what,
            target: meta.target().into(),
            name: meta.name().into(),
            level: meta.level().to_string(),
            fields: fields.0,
        });
    }
}

impl Subscriber for Recorder {
    fn register_callsite(&self, meta: &'static Metadata<'static>) -> Interest {
        if (self.select)(meta) {
            Interest::always()
        } else {
            Interest::never()
        }
    }
    fn enabled(&self, meta: &Metadata<'_>) -> bool {
        (self.select)(meta)
    }
    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut fields = Fields::default();
        attributes.record(&mut fields);
        self.push("span", attributes.metadata(), fields);
        self.state
            .lock()
            .unwrap()
            .spans
            .insert(id, attributes.metadata().target().into());
        Id::from_u64(id)
    }
    fn record(&self, span: &Id, values: &Record<'_>) {
        let mut fields = Fields::default();
        values.record(&mut fields);
        let mut state = self.state.lock().unwrap();
        let target = state
            .spans
            .get(&span.into_u64())
            .cloned()
            .unwrap_or_default();
        state.seen.push(Seen {
            what: "update",
            target,
            name: String::new(),
            level: String::new(),
            fields: fields.0,
        });
    }
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        self.push("event", event.metadata(), fields);
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

impl Capture {
    pub fn dispatch(&self) -> tracing::Dispatch {
        self.dispatch_selecting(|_| true)
    }

    /// A subscriber interested only in the callsites `select` accepts (a name, kind or field
    /// filter, as an application's own subscriber may have).
    pub fn dispatch_selecting(&self, select: fn(&Metadata<'_>) -> bool) -> tracing::Dispatch {
        tracing::Dispatch::new(Recorder {
            state: self.0.clone(),
            next: AtomicU64::new(1),
            select,
        })
    }

    /// Everything recorded with target `concord` (Concord's own output), every field.
    pub fn concord(&self) -> Vec<Seen> {
        let state = self.0.lock().unwrap();
        state
            .seen
            .iter()
            .filter(|s| s.target.starts_with("concord"))
            .cloned()
            .collect()
    }

    /// The raw text of every Concord record (events, spans, updates).
    pub fn raw(&self) -> String {
        format!("{:?}", self.concord())
    }

    /// The raw text of every record of every target (reqwest's stack included).
    pub fn raw_all(&self) -> String {
        format!("{:?}", self.0.lock().unwrap().seen)
    }

    pub fn assert_empty(&self) {
        let concord = self.concord();
        assert!(concord.is_empty(), "{concord:?}");
    }

    pub fn assert_call(&self, operation: &str, method: &str, route: &str, status: Option<u16>) {
        let concord = self.concord();
        assert!(
            concord.iter().all(|s| s.what == "event"),
            "J4 value-bearing spans are not part of J1: {concord:?}"
        );
        assert_eq!(concord.len(), 1, "{concord:?}");
        let event = &concord[0];
        assert_eq!(event.target, "concord");
        assert_eq!(event.name, "concord.call");
        assert_eq!(event.level, "INFO");
        let mut actual = event.fields.clone();
        // Real elapsed time is deliberately not a snapshot value. Its TYPE is part of the schema.
        assert!(matches!(
            actual.remove("elapsed_ms"),
            Some(Value::Unsigned(_))
        ));
        let mut expected = BTreeMap::from([
            ("operation".into(), Value::Text(operation.into())),
            ("method".into(), Value::Text(method.into())),
            ("route".into(), Value::Text(route.into())),
            (
                "delivery".into(),
                Value::Text(
                    if status.is_some() {
                        "answered"
                    } else {
                        "unknown"
                    }
                    .into(),
                ),
            ),
        ]);
        if let Some(status) = status {
            expected.insert("status".into(), Value::Unsigned(status.into()));
        } else {
            expected.insert("transport".into(), Value::Text("failed".into()));
        }
        // Equality, not a subset check: URLs, error strings, headers and extra fields fail here.
        assert_eq!(actual, expected);
    }

    pub fn assert_absent(&self, canaries: &[&str]) {
        let raw = self.raw();
        for canary in canaries {
            assert!(
                !raw.contains(canary),
                "raw subscriber received a canary: {raw}"
            );
        }
    }
}
