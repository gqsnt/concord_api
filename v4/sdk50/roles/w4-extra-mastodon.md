## Slice-specific notes

The host is the caller's instance, chosen at run time; a token for one instance must never be
sent to another. The streaming API may live on a different host than the REST API (discover it
from the instance). The public timeline stream is Server-Sent Events: an event, a multi-byte
UTF-8 character or a `data:` field can be split across chunks; comments and heartbeats are not
events. A disconnect is surfaced to the caller — no automatic replay or exactly-once claim.
