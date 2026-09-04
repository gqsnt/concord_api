# Counterbalanced variants

Use the same contract and task inputs in both variants. Randomize starting
variant by participant and switch variants after task 3.

## Variant E — exact candidate surface

- `set_x(Option<T>)`
- `omit_x()`
- `reset_x()` only for initialized slots
- explicit `PageLimit::Prefix(n)` and `PageLimit::CompleteWithin(non_zero_n)`

This is the release-candidate surface.

## Variant W — controlled `with_x` comparison

On an isolated study-only branch, add `with_x(T)` beside the Variant E methods.
Do not merge this branch. It exists only to measure tasks 1–3 and autocomplete
noise. All other syntax and generated docs must match Variant E.

Record the two commit IDs. A participant must not be told which is preferred.
The comparison is invalid if error text, examples, or unrelated behavior differs.
