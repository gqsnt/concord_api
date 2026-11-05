# Provenance: consecutive JSON value seeds

Synthetic codec seeds copied byte for byte from the 4.4 extensions handoff
(`fixtures/json_values/`, described in its `fixtures/README.md` and `fixtures/cases.json`, schema
version 1). They were authored for the handoff; none is a captured Kubernetes response, a live
interoperability result or personal/production data.

| File | Case | SHA-256 | Documented expectation | Asserted by |
|---|---|---|---|---|
| `multiline.json` | V01 | `4b5701a1e1cecfe1aff662d0ca0c08e286165583ad032de26e3ac48dbd446bd4` | two watch-style events, the first spanning four lines, no final LF | `json_values_framing::seeds_match_documented_expectations` |
| `scalars.json` | V02 | `9fd4acebdbcb7c63a8bc35757c47112a626f10f8776422af85a967a185e73f5f` | `12`, `true`, `"東京"`, `[1,2]`, `null`, `{"k":3}` separated by single spaces | same |
| `truncated.json` | V03 | `5029500f1c114d15165d935db516a4f065e37798aa7692ce6aad49a73a69ca2b` | `invalid_json_value` error after prefix `12` (input ends inside an object) | same |

The test embeds each file and compares it with the exact expected bytes written inline, then
reads it under every single split, every pair of split points and representative multi-split
partitions. `invalid_json_value` maps to `ReadErrorKind::Format`, label "the input ends inside a
JSON value".
