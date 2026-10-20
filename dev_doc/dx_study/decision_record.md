# Gate 3 decision record

**Status:** v3 protocol decision, revised 2026-10-16.

## Selected v3 surface

The value matrix selects payload `set_x`, exact `replace_x(Option<T>)`,
conditional `set_x_if_some(Option<T>)`, optional-only `omit_x()` and
initialized-only `reset_x()`. Reset restores the retained initial state.
Required uninitialized scope/endpoint constructor inputs have no setters.
The pagination terminals are `collect_all`, `collect_prefix` and `pages`.
The protocol evaluates this selected surface, including nested optional values;
it does not reopen the language design.

## Current evidence state

Zero qualifying humans have completed the protocol, so comprehension remains
Unknown and Gate 3 is Unknown/Blocked for public compatibility freeze. This
does not block integration of the unpublished `3.0.0-beta.1` after its
technical, documentation, Windows, and Linux gates pass.

## Stop rule

Only the thresholds in `rubric.md`, followed by fixes and successful retests of
all Critical and release-relevant Major findings, can support a human
comprehension claim. AI review, implementer familiarity, compiler tests, and
Rustdoc generation do not substitute for this evidence. The owner decides
whether that claim is mandatory for stable public freeze.
