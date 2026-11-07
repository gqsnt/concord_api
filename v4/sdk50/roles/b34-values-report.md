Left **B** in the working tree; no commit.

The [full report](docs/experiments/b34-values.md) covers syntax, generated signatures, API growth, guarantees, counterexamples and validation. [A is preserved as a patch](target/tmp/b34-values/representation-a.patch).

SDK production-line changes:

| SDK | A net | B net |
|---|---:|---:|
| QuickBooks | 0 | 0 |
| PayPal | 0 | −10 |
| Adyen | 0 | +10 |
| Square | −1 | −1 |
| Xero | −8 | +6 |
| **Total** | **−9** | **+5** |

A compiles, but cannot complete the generic policy ports: `Endpoint<T>` exposes no concrete getters, and Adyen’s sensitive reference must have no getter.

I recommend B’s explicit bindings for further policy work and the output-independent `operation()` method. Local getters deserve separate consideration. B removes the witnessed readbacks and PayPal’s allowlist, but **does not guarantee every future write declares a request ID**.

Retained B passes tests/doctests, Clippy and rustdoc with warnings denied, formatting, and diff checks. Existing assertions are unchanged.