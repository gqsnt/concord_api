# concord-extras

Compatibility facade for [concord-codecs](https://crates.io/crates/concord-codecs). All public
items and modules are re-exported from that crate, so existing paths name the same types. New
consumers can depend directly on `concord-codecs`.

Features `json`, `form`, `stream`, `csv`, `jsonl`, `multipart`, and `sse` forward to the
corresponding `concord-codecs` features. None is enabled by default.
