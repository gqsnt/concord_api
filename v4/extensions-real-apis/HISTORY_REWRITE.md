# History rewrite before the first push (push protection)

The first push of `concord-4.4-extensions-evidence` was refused by GitHub push protection: the
Stripe maintenance exercise's tests used a synthetic key constant shaped like a Stripe test
secret key (`sk_test_` followed by 24 alphanumerics) in two files, from the exercise's stage-1
commit on. The value was never a real credential. Rather than allowing it through the
repository's push-protection bypass (a security decision for the owner), the coordinator
rewrote the then-unpublished commits `origin/main..concord-4.4-extensions-evidence` (98 commits)
with `git filter-branch --tree-filter`, replacing the value with `sk_test_SYNTHETIC_maintenance`
in `checks/maintenance-stripe/tests/before.rs` and `checks/maintenance-stripe/tests/after.rs` wherever they existed. The
frozen consumer therefore stays byte-identical across its stages.

`git diff` between the old tip `54f96abe` and the new tip `45fd8cab` touches exactly those two
lines. Commits that did not descend from the Stripe stage-1 commit kept their SHAs. The 25
rewritten commits (old → new); records citing them were updated to the new SHAs:

| Old | New | Subject |
|---|---|---|
| `6a7ed00521b9` | `5a7259095079` | maintenance stripe: frozen initial consumer |
| `f90923e4b310` | `fb519c735546` | maintenance stripe: SDK change — customer invoice_settings[default_payment_method] and typed invoice_settings.default_payment_method expansion |
| `ca6c7b0cda69` | `9e9da9d82de4` | maintenance stripe: updated consumer |
| `3dcb3ac96dca` | `247d45c18264` | maintenance stripe: record stage-3 SHA and compare with the SDK's FRICTION.md |
| `081a13caddfe` | `883e3a396f7d` | Merge branch 'ext44/maint-stripe' into concord-4.4-extensions-evidence |
| `873ba383c0c4` | `402e5258714c` | Merge branch 'ext44/maint-kubernetes' into concord-4.4-extensions-evidence |
| `0e14f15a181b` | `08649a3f866d` | Merge branch 'ext44/sdk-salesforce' into concord-4.4-extensions-evidence |
| `5a2cd0fa558e` | `d72e4699297c` | maintenance shopify: frozen initial consumer |
| `08a2db9be3f8` | `ee7145adf4d9` | maintenance shopify: SDK change — CatalogProducts onlineStoreUrl on request, bulk product metafields |
| `2166f81ea54b` | `183959fa34cf` | maintenance shopify: updated consumer |
| `4f2f664164ca` | `993e2253d26a` | maintenance shopify: record findings |
| `32fa1136db50` | `6b554205c745` | maintenance salesforce: frozen initial consumer |
| `42f1936bba3b` | `17760187a21c` | Merge branch 'ext44/maint-shopify' into concord-4.4-extensions-evidence |
| `fea512c7b903` | `6ebd486a6275` | Merge branch 'ext44/dx03-csv' into concord-4.4-extensions-evidence |
| `3d6a1706da34` | `72c3f8b9f2d6` | DX-03 (a) stage 1: sequential collecting Salesforce export consumer (Codex gpt-6-astra) |
| `b45fe6c3ac22` | `0190425c05ab` | maintenance salesforce: SDK change — HasOptedOutOfEmail in the Contact export |
| `dad33569401e` | `34339304d760` | maintenance salesforce: updated consumer |
| `962567ca0bd7` | `58b70522fc48` | maintenance salesforce: record |
| `05090aacb80a` | `094887c09560` | Merge branch 'ext44/maint-salesforce' into concord-4.4-extensions-evidence |
| `e0d03d824c77` | `07e051debd98` | Merge the 4.4 extension development history onto verified main (d180254) |
| `30f945f91908` | `0d5eb339abdc` | testserver: clippy manual_range_contains in the lifecycle control |
| `8e44e2df70c1` | `10df16e6d10f` | extras: ReadError::is_terminal; JSON Lines Yield mode reports which errors end the read |
| `ce970c6117ad` | `f893136afebe` | DX-03 (a) stage 2: discovered bounded fanout, staged incremental sink, resume (Codex gpt-6-astra) |
| `39a75f21eaad` | `c9d48395ca6f` | 4.4 extensions: model/settings ledger (requested vs observed) |
| `54f96abedc04` | `45fd8cabbfaf` | Merge branch 'ext44/dx03-fanout' into concord-4.4-extensions-evidence |

The pre-rewrite local branches (`ext44/development`, `ext44/maint-*`) were never pushed; results
measured on old SHAs are cited only where the tree is identical apart from these two test
lines, and the final gates run on the rewritten commits.
