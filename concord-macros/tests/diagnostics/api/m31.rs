concord::api! {
    pub client ObjectStore(region: String, credentials: Credentials) {
        read Xml;
        problem crate::model::S3Problem;
        /// Signs every request, including each page and every retry.
        middleware Signed::new(credentials.clone(), region.clone());

        part version(version_id?: String) {
            query { "versionId" = version_id; }
        }
        part conditions(
            if_match?: String,
            if_modified_since?: String,
            if_unmodified_since?: String,
        ) {
            headers {
                "if-match" = if_match;
                "if-modified-since" = if_modified_since;
                "if-unmodified-since" = if_unmodified_since;
            }
        }
        part write_headers(
            content_type?: String,
            cache_control?: String,
            storage_class?: String,
        ) {
            headers {
                "content-type" = content_type;
                "cache-control" = cache_control;
                "x-amz-storage-class" = storage_class;
            }
        }
        part listing(prefix?: String, delimiter?: String) {
            query { prefix; delimiter; }
        }

        GET list_buckets(
            max_buckets?: u32, continuation_token?: String, prefix?: String,
        ) -> BucketsPage {
            base "https://s3.amazonaws.com";
            query {
                "max-buckets" = max_buckets;
                "continuation-token" = continuation_token;
                prefix;
            }
            paginate by_cursor(continuation_token, BucketsPage::next);
        }

        scope bucket(url: Base) {
            base url;

            /// Failure preserves the status and region header; 404 is not a boolean.
            HEAD head();
            /// Omit configuration in us-east-1; elsewhere give LocationConstraint.
            PUT create(configuration?: CreateBucketConfiguration) {
                body create_bucket_body(configuration);
                idempotent false;
            }
            DELETE delete();

            GET list_objects(
                max_keys?: u32, continuation_token?: String,
                start_after?: String, fetch_owner?: bool, ..listing,
            ) -> ObjectsPage {
                query {
                    "list-type" = "2";
                    "encoding-type" = "url";
                    "max-keys" = max_keys;
                    "continuation-token" = continuation_token;
                    "start-after" = start_after;
                    "fetch-owner" = fetch_owner;
                }
                paginate checked_cursor(continuation_token, ObjectsPage::next);
            }
            /// Per-key failures remain in DeleteResult, even on HTTP 200.
            POST delete_objects(batch: DeleteBatch) -> DeleteResult {
                query { "delete" = ""; }
                body crate::wire::xml(batch);
            }
            GET list_multipart_uploads(
                max_uploads?: u32, key_marker?: String,
                upload_id_marker?: String, ..listing,
            ) -> UploadsPage {
                query {
                    "uploads" = "";
                    "encoding-type" = "url";
                    "max-uploads" = max_uploads;
                    "key-marker" = key_marker;
                    "upload-id-marker" = upload_id_marker;
                }
                paginate uploads_after(key_marker, upload_id_marker);
            }

            scope object(key: String) {
                path "{*key}";

                GET get(range?: String, ..version, ..conditions) -> ByteStream {
                    headers { "range" = range; }
                    read Stream;
                }
                GET get_if_changed(
                    etag: String, range?: String, ..version, ..conditions,
                ) -> Option<ByteStream> {
                    headers { "if-none-match" = etag; "range" = range; }
                    read Stream;
                    status { 304 => None }
                }
                HEAD head(..version, ..conditions) -> http::HeaderMap {
                    status { 2xx { headers, body } => checked_head(headers, body)? }
                }
                /// ETag is the value; send_full also keeps version/checksum headers.
                PUT put(file: Body, size: u64, ..write_headers) -> String {
                    headers { "content-length" = size; }
                    body file;
                    status { 2xx { headers } => headers.require("etag")?.to_owned() }
                    idempotent false;
                }
                PUT copy_from(
                    source_bucket: String, source_key: String,
                    source_version?: String, source_if_match?: String,
                    metadata_directive?: String, ..write_headers,
                ) -> CopyResult {
                    headers {
                        "x-amz-copy-source" = copy_source(source_bucket, source_key, source_version);
                        "x-amz-copy-source-if-match" = source_if_match;
                        "x-amz-metadata-directive" = metadata_directive;
                    }
                    status { 2xx { body } => copy_ok(body)? }
                    idempotent false;
                }
                DELETE delete(..version) {
                    idempotent version_id.as_ref().is_some_and(|id| id != "null");
                }
                POST create_multipart_upload(..write_headers) -> InitiatedUpload {
                    query { "uploads" = ""; }
                }

                scope upload(upload_id: String) {
                    query { "uploadId" = upload_id; }

                    PUT upload_part(part_number: u32, file: Body, size: u64) -> String {
                        query { "partNumber" = part_number; }
                        headers { "content-length" = size; }
                        body file;
                        status { 2xx { headers } => headers.require("etag")?.to_owned() }
                    }
                    POST complete(parts: CompleteUpload) -> CompleteResult {
                        body crate::wire::xml(parts);
                        status { 2xx { body } => complete_ok(body)? }
                    }
                    DELETE abort();
                    GET list_parts(
                        max_parts?: u32, part_number_marker?: u32,
                    ) -> PartsPage {
                        query {
                            "max-parts" = max_parts;
                            "part-number-marker" = part_number_marker;
                        }
                        paginate checked_cursor(part_number_marker, PartsPage::next);
                    }
                }
            }
        }
    }
}
fn main() {}

include!("../../support/models.rs");
