# Fixture provenance

Source checked 2026-11-06: [PokéAPI documentation, API v2](https://pokeapi.co/docs/v2).
The documentation identifies the version as v2 and does not state a dated schema
revision. No live provider resource was requested. All fixture bytes are
hand-authored synthetic test data, not downloaded examples and not conformance
samples. Files are pinned as bytes by `.gitattributes`.

| Fixture | Source section and transformation |
| --- | --- |
| `pokemon.json` | Pokemon endpoint and Pokemon type: hand-authored shortened Clefairy body inspired by the documented fields and example, with empty nested arrays and null sprite/cries values. |
| `page-first.json` | Resource Lists/Pagination, Named: synthetic Pokémon references and count; next has a changed limit and reordered parameters. `HOST` is replaced with the loopback host at runtime. |
| `page-last.json` | Same section: synthetic nonempty last page with `next: null`; loopback `HOST` replacement. |
| `page-empty.json` | Same section: synthetic empty terminal page with `next: null`; loopback `HOST` replacement. |
| `not-found.txt` | Synthetic 404 text; the documentation does not specify this error body. |
| `malformed.json` | Synthetic negative case: numeric `next` contradicts the documented URL/nullable shape. |

The wire tests also hand-author a missing-`next` JSON response, an HTML response,
a truncated JSON response, and a 503 text response. These are test inventions,
not documented provider error examples. Fixture trailing newlines are intentional.
