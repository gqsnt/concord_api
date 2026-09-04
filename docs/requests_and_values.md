# Requests, values, atoms, and public shaping

## Exact value law

`?` adds exactly one Concord-owned outer presence layer. The written Rust type is otherwise opaque.

| Declaration | Effective slot type | Meaning |
|---|---|---|
| `x: T` | `T` | required exact payload unless initialized |
| `x?: T` | `Option<T>` | optional slot around exact payload |
| `x: Option<T>` | `Option<T>` | required domain decision |
| `x?: Option<T>` | `Option<Option<T>>` | outer absence plus exact optional payload |
| `x: Vec<T>` | `Vec<T>` | one exact collection value |
| `x?: Vec<T>` | `Option<Vec<T>>` | optional exact collection value |

Aliases and qualification do not change behavior.

## Initializers and defaults

```concord-pseudocode
#[default]
page_size: PageSize;

page_size: PageSize = DEFAULT_PAGE_SIZE;

#[default]
page_size?: PageSize;
```

`#[default]` and `= ...` are mutually exclusive. Stable explicit values are bounded literals or simple const/static paths. No calls, methods, closures, blocks, macros, or arbitrary expressions.

Evaluation occurs once at the narrowest owner snapshot:

- client build;
- scope construction;
- endpoint intent construction.

`reset_x()` restores the retained value; it never reevaluates `Default` or the initializer.

## Sensitivity

```concord-pseudocode
#[sensitive]
tenant: TenantId;
```

The value remains public protocol data and participates in the real request and internal correctness identity. Derived atoms are redacted in automatic plans, errors, observers, docs, and debug output.

Body and credential data already have stronger separate information classes; `#[sensitive]` is for ordinary public values only.

## Owner-qualified references

```concord-pseudocode
client.tenant
regional.region
endpoint.item_id
```

A named scope reference points to that exact visible owner. No generic `scope.*`, `parent`, `self`, `vars`, or `ep` search/rebinding exists.

## `RequestAtom`

```text
pub trait RequestAtom {
    fn write_atom(
        &self,
        output: &mut BoundedAtomWriter<'_>,
    ) -> Result<(), AtomError>;
}
```

Contract:

- one bounded logical scalar atom;
- deterministic for an unchanged semantic value;
- prompt, finite, side-effect-free, and no external I/O;
- core owns output bounds;
- context-specific DNS/path/query/header code validates/encodes afterward;
- stable error code/category without value leakage.

Provide built-ins for strings, chars, booleans, integer/nonzero integer types, and references. Do not blanket-implement for `Display`, `Option`, collections, maps/sets, or floating point.

## Explicit atom adapters

```concord-pseudocode
path [endpoint.id via Base32Id];
query { "at" = endpoint.timestamp via Rfc3339; }
headers { "If-Modified-Since" = endpoint.timestamp via HttpDate; }
```

An adapter changes one scalar representation at one placement without changing the caller type. `DisplayAtom` may exist in extras as an explicit compatibility adapter.

## `concat`

```concord-pseudocode
concat ["item-", endpoint.id via Base32Id]
```

Creates one atom. It never creates several labels, segments, query values, or headers. Optional sources and `each` are illegal inside `concat`. Sensitivity of any dependency propagates to the complete atom.

## Fixed base and subdomains

Each client declares one static absolute HTTP(S) base. Reject missing host, userinfo, query, fragment, and unsupported scheme.

```concord-pseudocode
subdomain ["api"];
subdomain [regional.region];
subdomain replace ["special"];
subdomain off;
```

- ordinary form prepends local labels to inherited generated labels;
- `replace` drops inherited generated labels but retains the fixed base host;
- `off` uses the fixed base host;
- one atom is one ASCII DNS label;
- generated labels are rejected for IP-literal bases.

A proxy may change routing but not logical origin, auth trust, rate identity, or planning.

## Path

```concord-pseudocode
path ["v2", "items", endpoint.id];
path replace ["special", endpoint.id];
path off;
```

- ordinary form appends generated segments;
- `replace` drops inherited generated segments;
- `off` retains only the fixed base path;
- one atom is one segment;
- reject empty, `.`, `..`, slash, and backslash before percent encoding.

Optional/spread path segments, raw/pre-encoded paths, templates, wildcards, and trailing-slash language are outside v2.1.

## Ordered repeated values

`each` requires a semantic `OrderedValues` capability. It expands a present ordered sequence only in query/header emission positions.

```text
pub trait OrderedValues {
    type Item;
    type Iter<'a>: Iterator<Item = &'a Self::Item>
    where
        Self: 'a,
        Self::Item: 'a;

    fn ordered_values(&self) -> Self::Iter<'_>;
}
```

Provide ordered implementations such as `Vec<T>`, arrays, boxed slices, and deliberate domain sequence types. Do not blanket every `IntoIterator` or unordered set/map.

## Query patch language

```concord-pseudocode
query {
    "category" = endpoint.category;
    when_present "locale" = endpoint.locale_override;
    "tag" = each endpoint.tags;
    append "tag" = each endpoint.additional_tags via TagSlug;
    "fields" = endpoint.fields via CommaSeparated;
    remove "legacy";
}

query replace { /* local program from empty */ }
query off;
```

### Assignment

`"K" = emission;` removes all current generated occurrences of `K`, then emits the local scalar or sequence at this statement position. An absent optional therefore clears inherited values.

### Conditional override

`when_present "K" = optional_emission;` preserves current occurrences when the outer Concord slot is absent. When present, it replaces them. A present empty sequence explicitly clears.

### Append

`append "K" = emission;` preserves current occurrences and adds zero, one, or many local occurrences. Optional absence is a no-op.

### Remove, replace, off

- `remove` deletes every generated occurrence for the key;
- `query replace` starts from an empty generated query;
- `query off` emits no generated public query but does not disable private auth query placement.

Keys are static validated strings. Final pair order is deterministic.

## Public headers

Headers use the same words under case-insensitive identity:

```concord-pseudocode
headers {
    "X-Tenant" = client.tenant;
    when_present "X-Locale" = endpoint.locale_override;
    append "X-Tag" = each endpoint.tags;
    remove "X-Legacy";
}
```

Explicit repeated occurrences are allowed only for fields whose semantics permit them. Use a scalar list adapter when one combined value is required.

Generic public headers cannot own transport/hop-by-hop/framing fields, proxy fields, auth targets, `Accept`, `Accept-Encoding`, request `Content-Type`/`Content-Encoding`, or cookies. Conflicts are errors, never precedence.
