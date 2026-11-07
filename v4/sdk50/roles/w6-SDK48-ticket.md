Callers need to read one document and update it safely: `GET /<index>/_doc/<id>` returns the
document with its `_seq_no` and `_primary_term`, and indexing with
`PUT /<index>/_doc/<id>?if_seq_no=…&if_primary_term=…` applies only if the document has not
changed since (otherwise `409` with a version-conflict error). Read the get and index document
references and the optimistic concurrency control page, and cite them with the date:
- https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-get
- https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-index
- https://www.elastic.co/docs/reference/elasticsearch/rest-apis/optimistic-concurrency-control

Add the read (a missing document is a typed "not found", not an error document) and the
conditional write (the caller passes the sequence number and primary term it read; a conflict is
a typed error the caller can act on; a timed-out write is uncertain and never resent). Keep bulk
and point-in-time search unchanged for existing callers.
