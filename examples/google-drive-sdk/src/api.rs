//! Transport declaration. Domain validation and session state live in the public client.

use concord::multipart::{Multipart, MultipartBody};
use concord::reqwest::header::{AUTHORIZATION, CONTENT_RANGE};
use concord::{Encoded, EncodedBody, Json};

use crate::{ChangesPage, File, FilesPage, NewFile, StartPageToken};

concord::api! {
    pub(crate) client DriveApi(origin: concord::BaseUrl, #[sensitive] token: String) {
        base origin;
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;

        scope files {
            path "drive/v3/files";
            GET list(
                query("pageSize") page_size: u16,
                query fields: String,
                #[sensitive] query("pageToken") page_token?: String,
                query q?: String,
                query("orderBy") order_by?: String,
            ) -> FilesPage { query { "spaces" = "drive"; } }
            GET get(id: String, query fields: String) -> File { path "{id}"; }
            POST create(body metadata: NewFile, query fields: String) -> File {}
        }
        scope uploads {
            path "upload/drive/v3/files";
            POST multipart(body payload: MultipartBody, query fields: String) -> File {
                query { "uploadType" = "multipart"; }
                request Multipart;
            }
            POST initiate(
                body metadata: NewFile, query fields: String,
                header("x-upload-content-type") media_type: String,
                header("x-upload-content-length") length: u64,
            ) { query { "uploadType" = "resumable"; } }
            PUT transfer(
                body payload: EncodedBody,
                header(CONTENT_RANGE) range: String,
            ) -> File { request Encoded; }
        }
        scope changes {
            path "drive/v3/changes";
            GET start() -> StartPageToken {
                path "startPageToken";
                query { "fields" = "startPageToken"; }
            }
            GET list(
                #[sensitive] query("pageToken") page_token: String,
                query("pageSize") page_size: u16,
                query fields: String,
            ) -> ChangesPage {
                query { "spaces" = "drive"; "includeRemoved" = "true"; }
            }
        }
    }
}
