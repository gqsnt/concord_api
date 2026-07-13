# Endpoint I/O

Generated endpoints select a resolved request adapter and response adapter
through `concord_core::__private`. Generated code provides typed arguments and
descriptor facts; core owns encoding, native request construction, response
limits, collection, and decoding.

Buffered JSON, text, and byte recipes are reusable. `StreamBody` stays
streaming. `MultipartBody` delegates directly to `reqwest::multipart::Form`;
Concord does not build boundaries manually or route multipart through a common
universal body boundary. `StreamResponse` retains the native response and
enforces configured limits lazily.

Generated source must not reference retired compatibility modules or numeric private
modules, runtime-state containers, transport abstractions, retry classifiers,
authentication loops, body polling, or response collectors.
