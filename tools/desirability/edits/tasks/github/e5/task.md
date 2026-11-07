# github / e5: one provider document field (nearest variant: a response document)

**Provider changelog.** Repository label objects (`GET /repos/{owner}/{repo}/labels` and every
other place a full label object is returned) now include a boolean `archived` field. Label
objects produced before the change, and GitHub's published examples, omit it: absent means
`false`. Labels embedded in issues are unchanged. Nothing else changed.

Public names the hidden check uses:
- `models::Label::archived: bool` (public field), `true` only when the document says so.
