Glue, hand-counted code lines: Salesforce 7 removed / 8 added (net -1); Datadog 6 / 37 (net -31); Frankfurter 0 / 0.
Counts exclude imports/docs/declarations/tests; negative net means more glue; exact ledger: GLUE.md.
New concepts: base <value>; public checked BaseUrl via standard AsRef; Datadog Destination; M3 Origin only in tests.
C1: the base names the provider/deployment value; there are no generator-control statements.
C2: URL facts enter the declaration; validation, link trust, credentials and send behavior stay in Rust.
C3: simple control unchanged: Frankfurter's complete src/lib.rs is byte-identical and keeps with_base_url.
C4: base binding is explicit; typed setters retain current rules, so fixed identity still requires separate C1.
C5: wrong/missing/optional base diagnostics must point to the declaration; compiler cases are specified in TESTS.md.
C6: one base fact per client; custom deployments are Destination::at(site, base).
C7: caller reqwest::Client, prepare, into_parts, send_raw and forward/reverse proxies remain available.
C8: deterministic checked values; M3 routing belongs to each test's client, with no SDK middleware or retries.
C9: compile-fail doctests, per-site M3 wire designs and existing canaries retain the guarantees; none was executed.
Wrappers stay: the large deletion estimate depends on C1/G1; C2 alone yields no net glue reduction here.
Does the declaration read better than before? Yes: instance/destination replaces a fictitious or arbitrary literal host; total Rust grows.
No compilation or Concord implementation; TESTS.md lists construction migrations and preserved assertions.
