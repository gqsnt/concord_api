Callers need their trade history: `POST /0/private/TradesHistory` (signed, like the existing
private reads) returns trades keyed by trade id with a total `count`, filtered by `start`/`end`
(timestamps or trade ids) and paginated with the `ofs` offset (50 results per call). Read the
trade-history reference and the authentication guide and cite them with the date:
- https://docs.kraken.com/api/docs/rest-api/get-trade-history
- https://docs.kraken.com/api/docs/guides/spot-rest-auth

Add a trade-history traversal that walks `ofs` until `count` is reached (with a page cap and a
resume point; the same filters on every page), signs every page with a fresh nonce from the
shared nonce source, and keeps the trades of completed pages when a later page fails. Keep market
discovery, the ticker and the balance / open-order reads unchanged for existing callers.
