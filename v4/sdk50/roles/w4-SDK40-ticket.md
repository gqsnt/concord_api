Callers need to create a DNS record in a zone: `POST /zones/{zone_id}/dns_records` with the
record's type, name, content, TTL and (where the type allows) `proxied`; the envelope's `result`
is the created record. Read the create-record reference and cite it with the date:
- https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/create/

Add the create so a caller gets the created record or a typed provider error (an envelope with
`success: false`, including "record already exists", is never a success); inputs outside the
documented constraints are refused before any request; a timed-out create is uncertain (the
caller lists the zone's records to reconcile) and is never resent automatically, including by the
SDK's opt-in 429 retry. Keep zone and record listing and retrieval unchanged for existing callers.
