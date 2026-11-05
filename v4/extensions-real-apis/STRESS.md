# Generated read/write workloads and lifecycle measurements (DATA-02, DATA-04)

Harness: `checks/stream-stress` (public paths only: an `api!` declaration, `Prepared::send_raw`,
the `concord::jsonl`/`concord::csv` incremental readers; uploads through the static
`Encode<StreamRecords<_>>` / `Encode<StreamRows<_>>` of declared endpoints). The server is the
owned loopback `examples/testserver`: bodies are generated chunk by chunk (`Body::Generate`,
`Body::GenerateThenFail`) and request bodies are counted without being stored. Nothing is
materialized on either side.

Command (release build, 4 worker threads):

```bash
CONCORD_SOURCE_SHA=$(git rev-parse HEAD) cargo run --release -p concord-check-stream-stress -- --mib 256 --out stress.json
```

Recorded run (the table): [evidence/stress-256-6f6ac84.json](evidence/stress-256-6f6ac84.json),
source `6f6ac847` (the final implementation candidate), Linux x86_64, rustc 1.99.1, 20 cores, no
other build running (load average 0.8 when it started; throughput is still one machine's).
Whole run: 8.5 s wall, peak RSS 10.5 MiB
([evidence/stress-256-6f6ac84.time.txt](evidence/stress-256-6f6ac84.time.txt)). The table is
generated from the JSON (the generator reproduces the earlier table from the earlier JSON
exactly). An earlier run at `54f96abe`, before the review fixes, with other agents' builds
running ([evidence/stress-256-54f96ab.json](evidence/stress-256-54f96ab.json)), gave the same
records, bytes, retained bytes, source polls, errors and cancellation invariants; only the
timing-dependent counts of the cancel scenarios differ (records received before the 500 ms
upload cancel; bytes generated before the CSV reader was dropped).

Fixed settings: records of exactly 128 bytes (JSON Lines; the same records as CSV are 128 bytes
on download and 99 bytes on upload), `max_record_bytes` 4096, generator chunks of 64 KiB,
deterministic records (the seed is the index: `rec(i)`), CSV `HasHeader`/comma/CRLF. N = 256 MiB
for reads (2,097,152 records), N/4 = 64 MiB for uploads.

| Scenario | Records delivered | MiB consumed / received | First record (ms) | Body finished (ms) | Total (ms) | Peak library-retained bytes | Peak in flight (MiB) | Peak process RSS (MiB) | Source polls before send | Source polls total | Error | Progress at cancel / +200 ms |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| jsonl-read-N | 2,097,152 | 256.0 | 0.34 | 325 | 326 | 127 | 1.2 | 9.7 | – | – | – | – |
| jsonl-read-2N | 4,194,304 | 512.0 | 0.21 | 650 | 650 | 127 | 2.0 | 10.6 | – | – | – | – |
| jsonl-read-slow-consumer | 524,288 | 64.0 | 0.22 | 364 | 391 | 127 | 5.1 | 11.1 | – | – | – | – |
| jsonl-read-cancel | 100,000 | 12.2 | 4.91 | – | 19 | 127 | 2.5 | 10.9 | – | – | – | 12976128/12976128 |
| jsonl-read-late-failure | 1,048,576 | 128.0 | 5.33 | 167 | 167 | 127 | 2.5 | 10.8 | – | – | Transport | – |
| jsonl-upload-N | 524,288 | 64.0 | – | – | 84 | 0 | 0.0 | 10.8 | 0 | 524288 | – | – |
| jsonl-upload-2N | 1,048,576 | 128.0 | – | – | 168 | 0 | 0.0 | 10.9 | 0 | 1048576 | – | – |
| jsonl-upload-slow-receiver | 131,072 | 16.0 | – | – | 1325 | 0 | 0.0 | 10.9 | 0 | 131072 | – | – |
| jsonl-upload-cancel | 28,424 | 3.5 | – | – | 501 | 0 | 0.0 | 10.9 | 0 | 36608 | – | 36608/36608 |
| jsonl-upload-late-error | 262,144 | 32.0 | – | – | 43 | 0 | 0.0 | 10.9 | 0 | 262145 | Transport | – |
| csv-read-N | 2,097,152 | 256.0 | 0.23 | 433 | 433 | 384 | 0.9 | 11.0 | – | – | – | – |
| csv-read-2N | 4,194,304 | 512.0 | 0.24 | 917 | 917 | 384 | 2.9 | 11.0 | – | – | – | – |
| csv-read-slow-consumer | 524,288 | 64.0 | 0.24 | 391 | 413 | 384 | 5.0 | 11.3 | – | – | – | – |
| csv-read-cancel | 100,000 | 12.2 | 4.98 | – | 30 | 384 | 2.5 | 11.1 | – | – | – | 12976147/12976147 |
| csv-read-late-failure | 1,048,576 | 128.0 | 0.25 | 221 | 221 | 384 | 0.7 | 11.1 | – | – | Transport | – |
| csv-upload-N | 524,288 | 49.5 | – | – | 87 | 0 | 0.0 | 11.1 | 0 | 524288 | – | – |
| csv-upload-2N | 1,048,576 | 99.0 | – | – | 166 | 0 | 0.0 | 11.1 | 0 | 1048576 | – | – |
| csv-upload-slow-receiver | 131,072 | 12.4 | – | – | 1010 | 0 | 0.0 | 11.1 | 0 | 131072 | – | – |
| csv-upload-cancel | 36,751 | 3.5 | – | – | 501 | 0 | 0.0 | 11.1 | 0 | 47310 | – | 47310/47310 |
| csv-upload-late-error | 260,288 | 24.6 | – | – | 46 | 0 | 0.0 | 11.1 | 0 | 262145 | Transport | – |

## What these show, and their scope

- **No whole-body buffering on reads.** At N (256 MiB) and 2N (512 MiB) the reader's
  library-owned buffer (`peak_retained_bytes`: the unfinished record) peaks at 127 bytes for JSON
  Lines and 384 bytes for CSV — identical at both sizes and far below the 4 KiB record limit —
  and the **whole process** peaks at about 11–12 MB RSS in both cases. The required read point
  (≥ 256 MiB, ≥ 10× the effective window) is exceeded: the effective library window is one
  record (≤ 4 KiB) plus the current transport chunk; the generated-minus-consumed in-flight
  window (socket and transport buffering between server and reader) peaks at 0.7–5.1 MiB.
- **First record before the body ends.** First records arrive in 0.2–0.4 ms in the full-body
  read scenarios and in about 5 ms in three of the four cancel/late-failure scenarios (also in
  the earlier run for CSV cancel; the cause was not investigated); the server finishes
  producing a full body after 0.33–0.92 s.
- **Backpressure.** A consumer pausing 5 ms every 10,000 records keeps the in-flight window at
  about 5 MiB: the server stops producing while the reader does not read.
- **Cancellation.** Dropping a reader after 100,000 records stops generation: the generated byte
  count is the same at the drop and 200 ms later, and the server sees its body end. Cancelling an
  upload send after 500 ms against a slow receiver stops polling the source (equal counts at the
  cancel and 200 ms later).
- **Late failures.** A connection dropped after half the records delivers exactly that prefix,
  then a terminal `Transport` error. An upload source failing at record 262,144 is polled 262,145
  times (each record once, then the failure) and the send fails with a transport error; the
  receiver holds only whole records (JSON Lines: 262,144; CSV: 260,288 — the up to 16 KiB frame
  being assembled when the source failed is not sent).
- **One-shot uploads.** Every upload polled its source zero times during preparation and exactly
  once per record when sent.

Not shown: allocation inside user `Deserialize`/`Serialize` code, reqwest's or hyper's internal
buffers (only bounded indirectly by the in-flight window), Windows memory (RSS sampling uses
`/proc`; the harness's smoke tests run on both platforms in CI), and live provider behaviour.
CI-sized versions of every workload with invariant assertions run in the maintained gate
(`checks/stream-stress/tests/smoke.rs`).
