concord::api! {
    /// An object store that speaks the S3 protocol: Amazon S3, or a store that copies it
    /// (MinIO, Ceph, Cloudflare R2, …).
    ///
    /// **Every request must be signed** (Signature Version 4). Make the client with
    /// [`S3::signed`] for AWS, or [`S3::compatible`] for another store. `S3::new` compiles,
    /// and gives a client that does not sign: S3 answers it `403`.
    pub client S3(
        /// The region, as AWS writes it: `eu-west-1`.
        region: Region,
    ) {
        base "https://s3.{region}.amazonaws.com";
        read Xml;
        problem S3Problem;

        /// How the object is stored and served back: on `put`, `copy_from`, `create_multipart_upload`.
        part object_headers(content_type?: String, cache_control?: String,
                            content_disposition?: String, content_encoding?: String,
                            storage_class?: StorageClass) {
            headers {
                "content-type" = content_type;
                "cache-control" = cache_control;
                "content-disposition" = content_disposition;
                "content-encoding" = content_encoding;
                "x-amz-storage-class" = storage_class;
            }
        }

        /// ListBuckets: the buckets of the account. In pages since 2024 (unsure); a store
        /// that does not page sends one page with no token.
        GET list_buckets(prefix?: String, bucket_region?: Region,
                         max_buckets?: u32, continuation_token?: String) -> BucketList {
            query { prefix; "bucket-region" = bucket_region; "max-buckets" = max_buckets;
                    "continuation-token" = continuation_token; }
            paginate cursor(continuation_token, BucketList::next_token);
        }

        /// One bucket. Written in the path here; on AWS the signer puts it in the host
        /// (`{name}.s3.{region}.amazonaws.com`), unless the name has a dot. See [`Addressing`].
        scope bucket(name: String) {
            path "{name}";

            /// HeadBucket. `None` when there is no such bucket. One that exists and is not
            /// yours is a `403`, an error.
            HEAD head() -> Option<BucketHead> {
                status {
                    2xx { headers } => Some(BucketHead::from_headers(headers)),
                    301 { headers } => Some(BucketHead::from_headers(headers)),   // it lives in another region (unsure)
                    404 => None,
                }
            }

            /// CreateBucket, in the region of this client.
            PUT create(object_ownership?: ObjectOwnership) {
                headers { "x-amz-object-ownership" = object_ownership; }
                body bucket_configuration(client.region);
            }

            /// DeleteBucket. The bucket must be empty.
            DELETE delete();

            /// ListObjectsV2: the objects in the order of their keys, a page at a time. With a
            /// `delimiter`, the keys that share a prefix up to it come once, in `common_prefixes`.
            GET list_objects(prefix?: String, delimiter?: String, max_keys?: u16,
                             start_after?: String, fetch_owner?: bool,
                             continuation_token?: String) -> ListBucketResult {
                query { "list-type" = "2"; prefix; delimiter; "max-keys" = max_keys; "start-after" = start_after;
                        "fetch-owner" = fetch_owner; "continuation-token" = continuation_token; }
                paginate |call, page| match (page.value().is_truncated, &page.value().next_continuation_token) {
                    (true, Some(token)) => Ok(Some(call.continuation_token(token.clone()))),
                    (true, None) => Err(concord::mismatch("IsTruncated without a NextContinuationToken")),
                    (false, _) => Ok(None),
                };
            }

            /// DeleteObjects: up to 1000 keys in one request. A key that could not be deleted is
            /// in `errors`, and the call itself succeeds.
            POST delete_objects(keys: Vec<String>, quiet?: bool) -> DeleteResult {
                query { "delete" = ""; }
                headers { "content-md5" = DeleteRequest::new(keys, *quiet).content_md5(); }
                body xml(&DeleteRequest::new(keys, *quiet));
                idempotent;
            }

            /// ListMultipartUploads: uploads that were started and neither completed nor aborted.
            GET list_multipart_uploads(prefix?: String, max_uploads?: u16,
                                       key_marker?: String,
                                       upload_id_marker?: UploadId) -> ListMultipartUploadsResult {
                query { "uploads" = ""; prefix; "max-uploads" = max_uploads;
                        "key-marker" = key_marker; "upload-id-marker" = upload_id_marker; }
                paginate |call, page| page.value().next_markers()
                    .map(|(key, id)| call.key_marker(key).upload_id_marker(id));
            }

            /// One object. Its key may hold `/`: `photos/2026/cat.jpg`. A key that ends with
            /// `/`, or holds `//`, cannot be sent (finding 3).
            scope object(key: String) {
                path "{*key}";

                /// GetObject: the head, and the bytes as they come. With `range`, a part of them
                /// (`206`). `idle_timeout` goes on while you read.
                GET get(range?: ByteRange, if_match?: ETag, version_id?: VersionId) -> Download {
                    query { "versionId" = version_id; }
                    headers { range; "if-match" = if_match; }
                    read Stream;
                    status { 2xx { body, headers } => Download::new(body, headers)? }
                }

                /// GetObject with `If-None-Match`: `None` while the object still has this ETag (`304`).
                GET get_if_changed(etag: ETag, range?: ByteRange) -> Option<Download> {
                    headers { "if-none-match" = etag; range; }
                    read Stream;
                    status {
                        2xx { body, headers } => Some(Download::new(body, headers)?),
                        304 => None,
                    }
                }

                /// HeadObject: the head that `get` gives, with no bytes. `None` when there is no
                /// such key. (A caller without `s3:ListBucket` gets `403` instead: S3 hides which.)
                HEAD head(version_id?: VersionId) -> Option<ObjectHead> {
                    query { "versionId" = version_id; }
                    status {
                        2xx { headers } => Some(ObjectHead::from_headers(headers)?),
                        404 => None,
                    }
                }

                /// PutObject: up to 5 GiB in one request. A stream is sent once, and needs
                /// `content_length`: S3 refuses a chunked body. `.if_none_match("*")` writes only
                /// if there is no object at this key (`412` otherwise). User metadata: `.meta(…)`.
                PUT put(content: Body, content_length?: u64, if_none_match?: String,
                        ..object_headers) -> Stored {
                    headers { "content-length" = content_length; "if-none-match" = if_none_match; }
                    body content;
                    status {
                        2xx { headers } => Stored {
                            etag: ETag::new(headers.require("etag")?),
                            version_id: headers.get("x-amz-version-id").map(VersionId::new),
                        },
                    }
                }

                /// CopyObject: this object, made on the server from `source` (5 GiB at most).
                /// A `200` can still carry an error; it is read, and is an error here too.
                PUT copy_from(source: CopySource, metadata_directive?: MetadataDirective,
                              source_if_match?: ETag, ..object_headers) -> CopyObjectResult {
                    headers {
                        "x-amz-copy-source" = source;
                        "x-amz-metadata-directive" = metadata_directive;
                        "x-amz-copy-source-if-match" = source_if_match;
                    }
                    status { 200 { body: reply } => XmlReply::into_result(reply)? }
                }

                /// DeleteObject. A key that is not there is "deleted" too: `204` either way.
                DELETE delete(version_id?: VersionId) {
                    query { "versionId" = version_id; }
                }

                /// CreateMultipartUpload. Each call starts a new upload, kept (and billed) until it
                /// is completed or aborted; so it is never sent twice.
                POST create_multipart_upload(..object_headers) -> InitiateMultipartUploadResult {
                    query { "uploads" = ""; }
                }

                /// One multipart upload of this object.
                scope multipart(upload_id: UploadId) {
                    query { "uploadId" = upload_id; }

                    /// UploadPart: part `part_number` (1 to 10 000; 5 MiB at least, but the last).
                    /// Bytes held in memory can be sent again; a stream is sent once.
                    PUT upload_part(part_number: u16, part: Body, content_length?: u64) -> CompletedPart {
                        query { "partNumber" = part_number; }
                        headers { "content-length" = content_length; }
                        body part;
                        status { 2xx { headers } => CompletedPart { part_number: *part_number, etag: ETag::new(headers.require("etag")?) } }
                    }

                    /// CompleteMultipartUpload: the object, made of these parts in this order. It
                    /// can take minutes (S3 sends spaces meanwhile), and a `200` can carry an error.
                    POST complete(parts: Vec<CompletedPart>) -> CompleteMultipartUploadResult {
                        body xml(&CompleteMultipartUpload { parts });
                        status { 200 { body: reply } => XmlReply::into_result(reply)? }
                        idempotent;     // the same parts make the same object (unsure: a repeat after success may say NoSuchUpload)
                    }

                    /// AbortMultipartUpload: the parts are dropped, and no longer billed.
                    DELETE abort();

                    /// ListParts: the parts uploaded so far.
                    GET list_parts(max_parts?: u16, part_number_marker?: u16) -> ListPartsResult {
                        query { "max-parts" = max_parts; "part-number-marker" = part_number_marker; }
                        paginate cursor(part_number_marker, ListPartsResult::next_marker);
                    }
                }
            }
        }
    }
}
