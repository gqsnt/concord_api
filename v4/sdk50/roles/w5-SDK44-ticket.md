Callers need to list a bucket's file names and delete a file version: `b2_list_file_names`
(paginated by `startFileName` / `nextFileName`, with `prefix`, `delimiter` and `maxFileCount`)
and `b2_delete_file_version` (by file name and file id). Read both references and cite them with
the date:
- https://www.backblaze.com/apidocs/b2-list-file-names
- https://www.backblaze.com/apidocs/b2-delete-file-version

Add a file-name traversal on the storage scope (the same prefix and delimiter on every page, a
page cap and a resume point, completion when `nextFileName` is null) and a version delete (a
timed-out delete is uncertain: the caller lists again; never resent automatically; a file id and
name that do not match is a typed provider error). Both use the account authorization and the
API URL issued for it. Keep authorization, upload and download unchanged for existing callers.
