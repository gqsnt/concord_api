# Independent first-use fixtures

All XML and binary files in this directory were hand-authored for `tests/first_use.rs` on
2026-11-06. They were not downloaded AWS response examples, captured traffic, or copied SDK
test fixtures. No requests were sent to AWS and no real credentials were used.

- `list-first.xml`, `list-empty-next.xml`, `list-last.xml`: synthetic ListObjectsV2 XML
  following [the response schema and continuation rules](https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html).
  The middle page intentionally has no items but has a next token. Keys are URL-encoded;
  tokens contain plus, slash, equals and a space to detect accidental interpretation.
- `list-missing-token.xml`, `list-repeated-token.xml`: intentionally broken pagination
  documents, derived from the hand-authored first page. They model a faulty provider.
- `slow-down.xml`, `precondition.xml`, `missing.xml`: synthetic S3 error envelopes for
  503, 412 and 404. Message/resource text and document request ids are deliberately distinct
  from the HTTP response headers to check preservation and default diagnostic redaction.
- `payload.bin`: 24 hand-selected bytes, including NUL, 0xff, CRLF and LF. Its contents are
  `b"\x00\xff\r\nfirst-use S3 bytes\x00\n"`; it has no text normalization or content checksum
  relationship to the opaque ETag used by the fake.

Response heads and chunked transfer/cutoff faults are constructed in the test using the public
testserver API. HEAD and 304 omit Content-Length and transmit no body. The main operation sources are
[HeadObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html),
[GetObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetObject.html), and
[PutObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html).
The independent request verifier implements the documented
[SigV4 authorization algorithm](https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sig-v4-header-based-auth.html)
using the existing dev-dependencies `sha2` and `hmac`. All credential strings are synthetic.

The SDK root's `.gitattributes` applies `fixtures/** -text` to these byte-sensitive files.
