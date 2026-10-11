# Requests, values, atoms and public shaping

This page explains how declared values become the public request head: the
URL (origin, DNS labels, path), the query string and public headers. The
normative rules are [DSL specification §6-§8](spec.md#6-values); this page is
the task view.

## Values and presence

A value's Rust type is its domain meaning; the DSL markers are its Concord
semantics:

| Declaration | Slot | Starts as | Caller sees |
|---|---|---|---|
| `x: T` | `T` | supplied by the caller | positional argument (scope/endpoint) or required builder setter (root) |
| `x?: T` | `Option<T>` | absent | `set_x`, `replace_x`, `set_x_if_some`, `omit_x` |
| `x: T = V` / `#[default] x: T` | `T` | retained initializer | `set_x`, `set_x_if_some`, `reset_x` |
| `x?: T = V` / `#[default] x?: T` | `Option<T>` | present retained initializer | all five controls |

An absent optional omits every keyed emission it feeds. A present empty `each`
sequence emits zero occurrences, which is different from an absent optional
sequence. An initialized value is re-selected by `reset_x()` without evaluating
the initializer again; defaults are evaluated once per owner boundary (builder
creation, scope selection, request construction).

Payload setters accept `impl IntoValue<T>`: identity, `&str -> String` and
`&String -> String`, nothing else. So `set_count(50)` works for `u32` through
integer inference, `set_category("hardware")` works for `String`, and a
validated domain newtype must be constructed explicitly.

## Where values go

A value reaches the wire in one of these ways:

- **Direct placement** on the declaration: `query tenant: String` (key
  `tenant`), `query("tag") each tags: Vec<String>` (key `tag`, one occurrence
  per item), `header("Accept-Language") language?: String`. An optional
  `via Adapter` after the type belongs to that placement.
- **Explicit block uses**: `query { "locale" = locale; }`,
  `headers { "X-Region" = regions.region; }`. The same value can be placed
  directly and used again in blocks; setting it sets every use.
- **Route components**: path template holes (`path "items/{id}"`) and DNS
  labels (`subdomain [region]`).
- **Rate dimensions** and **pagination mappings**, which are semantic uses
  without a wire key of their own.

A value with no semantic use is rejected (VALUE-06).

## References

In an owner's clause, a bare name is that owner's value. Reach an ancestor by
qualifying it with the owner: `client.tenant` or `regional.region`. A missing
bare name is an error even if an ancestor has the same name, and the
diagnostic suggests the qualified spelling rather than guessing.

## Atoms

An atom is one scalar wire value. Types implement `RequestAtom` to own their
canonical protocol spelling:

```text
impl RequestAtom for ApiVersion {
    fn write_atom(&self, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str(self.0)
    }
}
```

An explicit `AtomEncoder<T>` adapter chooses another spelling for one use, with
`via`:

```concord
header("X-Label") label: Option<&'static str> via LabelOrNone = crate::DEFAULT_LABEL;
```

The writer is bounded (65,536 bytes per formatted public value) and checks each
append before retaining it. An atom encoder cannot choose presence, repetition,
a native URL, a body, a retry or a credential. `concat [a, b]` builds one scalar
from total components; `each` repeats; neither can appear inside the other, and
`each` never appears in a path, DNS label or rate dimension.

## Origin, DNS labels and paths

The base fixes the scheme, host suffix, effective port and base path. DNS labels
and path segments are generated relative to it:

```concord
client RouteApi {
    base "https://route.example.test/root";
    values {
        api_version: String = "v2";
    }
    subdomain ["api"];
    path "{api_version}";

    scope regional(region: String) {
        subdomain [region];
        path "regions/{region}";

        GET item(id: String) -> () via Empty {
            path "items/{id}";
        }
        GET details(id: String) -> () via Empty {
            path "resource-{id}/details";
        }
        GET report(date: Day) -> () via Empty {
            path "reports/{date via IsoDate}";
        }
        GET legacy() -> () via Empty {
            path replace "legacy/{client.api_version}";
        }
        GET origin_root() -> () via Empty {
            path off;
        }
        GET braces() -> () via Empty {
            path "literal-{{braces}}";
        }
    }

    scope global() {
        subdomain replace ["global"];
        GET status() -> () via Empty;
    }

    scope apex() {
        subdomain off;
        GET ping() -> () via Empty;
    }
}
```

| Endpoint | Generated URL |
|---|---|
| `regional("eu").item("7")` | `https://eu.api.route.example.test/root/v2/regions/eu/items/7` |
| `regional("eu").details("7")` | `.../root/v2/regions/eu/resource-7/details` |
| `regional("eu").legacy()` | `https://eu.api.route.example.test/root/legacy/v2` |
| `regional("eu").origin_root()` | `https://eu.api.route.example.test/root/` |
| `regional("eu").braces()` | `.../root/v2/regions/eu/literal-%7Bbraces%7D` |
| `global().status()` | `https://global.route.example.test/root/v2` |
| `apex().ping()` | `https://route.example.test/root/v2` |

Each path segment is percent-encoded exactly once; user `%2F` is data, never a
separator. A segment may not be empty, `.` or `..`, or contain `/`, a backslash
or a literal `?`/`#`. Each DNS label is one lowercase ASCII label of 1-63
bytes; a region value such as `eu.west` or `EU_WEST` is rejected at planning
and execution, not split or repaired.

## Query and header programs

Every owner contributes to one ordered, symbolic program per family. At each
owner, Concord starts from the inherited program (or clears it for
`query replace { ... }`), adds direct placements in declaration order, then
applies block statements in source order. Nothing is encoded until the complete
program has decided which occurrences survive.

```concord
client PatchApi {
    base "https://patch.example.test/";
    values {
        locale: String = "fr";
    }
    query {
        "a" = "1";
        "locale" = locale;
        "z" = "9";
    }

    GET conditional(override_locale?: String) -> () via Empty {
        query { when_present "locale" = override_locale; }
    }
    GET exact(override_locale?: String) -> () via Empty {
        query { replace "locale" = override_locale; }
    }
    GET appended(extra_locale?: String) -> () via Empty {
        query { append "locale" = extra_locale; }
    }
    GET removed() -> () via Empty {
        query { remove "locale"; }
    }
    GET fresh(query page?: u32) -> () via Empty {
        query replace { "only" = "this"; }
    }
    GET bare() -> () via Empty {
        query off;
    }
}
```

| Request | Query |
|---|---|
| `conditional()` | `a=1&locale=fr&z=9` |
| `conditional().set_override_locale("de")` | `a=1&z=9&locale=de` |
| `exact()` | `a=1&z=9` |
| `exact().set_override_locale("de")` | `a=1&z=9&locale=de` |
| `appended().set_extra_locale("de")` | `a=1&locale=fr&z=9&locale=de` |
| `removed()` | `a=1&z=9` |
| `fresh().set_page(2)` | `page=2&only=this` |
| `bare()` | (none) |

A plain `"k" = e` over a key that an ancestor or earlier statement could emit
is a compile error naming both writers; choose `replace`, `when_present`,
`append` or `remove`. `replace`, `when_present` and `remove` themselves require
an earlier writer, which catches misspelled override targets. Query keys are
case-sensitive; header names compare case-insensitively (`x-region` and
`X-Region` are one key).

Because nothing is encoded before selection, a suppressed ancestor contribution
never runs its adapter: a failing adapter on a replaced key cannot fail the
request. A surviving scalar is encoded once per preparation, and each surviving
`each` item once, in order. Planning and execution are separate preparations and
may encode again, so adapters must be deterministic and side-effect-free.

## Sensitive values

`#[sensitive]` marks a public value that must reach the server but never
automatic output:

```text
values {
    #[sensitive] query tenant: TenantId;
}
```

The real request carries the value. Plans, error displays, retry classifiers,
observers, response metadata debug and generated request debug show the key but
not the value. A sensitive path segment is replaced as a whole; a sensitive DNS
label withholds the full host from automatic observers. Sensitivity of a
query/header key is sticky along the root-to-owner chain: a later public
`replace`, a removal or a family fence does not declassify it. Keys whose
normalized names contain `authorization`, `cookie`, `token`, `secret`,
`password`, `passwd`, `credential`, `session`, `api-key`, `apikey` or
`client-secret` are always redacted as well. None of this makes a value a
credential: credentials come only from `credentials` and `auth`.

## Header ownership

Transport, media, negotiation and authentication headers are not public
targets: `Host`, `Authorization`, `Proxy-Authorization`, `Proxy-Authenticate`,
`Accept`, `Accept-Encoding`, `Content-Type`, `Content-Encoding`,
`Content-Length`, `Transfer-Encoding`, `Connection`, `Keep-Alive`, `TE`,
`Trailer`, `Upgrade`, `Proxy-Connection`, `Cookie` and `Set-Cookie`, plus any
name claimed by an effective auth placement. Collisions are rejected at compile
time (or at runtime-endpoint bind) even when the optional source is absent.
Singleton request fields such as `User-Agent`, `Range` and `Origin` cannot be
repeated with `each` or `append`.

## Limits

The public head is bounded: 256 components, 256 headers, 65,536 aggregate header
bytes, 131,072 bytes for the logical URL and for its sanitized form, and 256
bytes for a static query/header name. Reaching a limit is a structured
preparation error; nothing is truncated.
