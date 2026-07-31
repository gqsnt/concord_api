# Planning and observation

- **Audience:** API callers, tooling authors, hook authors, and security reviewers
- **Purpose:** concept guide
- **Normative references:** [runtime model](runtime.md), [design invariants](design_invariants.md)

Concord exposes several purpose-specific views instead of one omniscient native
request or response object.

## Endpoint descriptions

`endpoint.description()` returns stable structural contract facts:

- endpoint identity and method;
- source and route shape;
- body category and specification identity;
- response acquisition mode;
- timeout selection;
- pagination, authentication, rate, and credential-source capabilities.

Descriptions contain no bound values, bodies, credentials, resolved rate-key
values, or runtime objects. They support inventory, documentation, metrics
registration, and policy review without a built client.

## Safe planning

`client.plan(&endpoint)` executes real public head shaping and validates:

- endpoint identity and method;
- base, host, path, query, and public headers;
- dynamic formatting and finite structural budgets;
- body shape without body work;
- timeout selection;
- effective policy structure.

Planning does not run hooks, body metadata/preparation, credential acquisition,
limiter authority, native request construction, HTTP, or completion. It returns
a credential-free logical URL and is not an authenticated request preview.

## Public versus private information

| Surface | Public application data | Credential material | Native objects |
|---|---:|---:|---:|
| description | structure only | no | no |
| safe plan | logical URL and public headers | no | no |
| before-send hook | sanitized public operation facts | no | no |
| limiter | safe request context and selected rate-key values | no | no |
| response hook | sanitized response head | no | no |
| retry classifier | finite reduced failure context | no | no |
| completion | identities, counts, timing, status, outcome | no | no |
| explicit accepted response accessors | response application data | no request credentials | no native response |

Public values may still be sensitive application data. The boundary guarantees
absence of DSL secret material and private native state, not universal logging
safety.

## Debug and errors

Automatic formatting exposes structure and safe categories rather than values.
Accepted response headers and decoded output are returned only through explicit
accessors. Rate-key values and credential material are suppressed from built-in
formatting and errors.

`ApiError` and `ClientBuildError` retain stable categories and safe kinds.
Native parser, transport, provider, codec, body, and hook causes are reduced or
dropped before crossing the public boundary.

## Purpose-specific extensions

Hooks, limiters, retry classifiers, body/response specifications, providers,
and completion receive different semantic contexts. The narrow context is a
security and maintainability property: an extension receives what its role
needs, not an unrestricted request lifecycle object.

## Review questions

- Could this value appear in a logical URL or public header?
- Is it acceptable in plans, hooks, metrics, and logs?
- Should the value instead be a secret or private credential placement?
- Does a custom extension store or log sensitive public values?
- Can a new observation API be expressed with a narrower semantic structure
  instead of exposing native transport?

## Related

- [Authentication](auth.md)
- [Execution policy](execution_policy.md)
- [Extensions](extensions.md)
