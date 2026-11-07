# Wave 2 status

Frozen library baseline: `8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f` (`sdk50/integration`): Wave 1
SDKs, the between-waves changes and both reviews' fixes. Gate on it: `scripts/verify.sh --package
--bench` 66/66 PASS, clean, 6m52s; `--nightly-only` PASS (16 tests). Authors started 2026-11-07
02:00 CET.

| SDK | Worktree / branch | Author (requested) | Status |
|---|---|---|---|
| SDK26 Postmark | `work/w2-sdk26-postmark`, `sdk50/w2-sdk26-postmark` | Codex `gpt-6.1-terra` xhigh | complete: author, consumer (Astra), maintainer (Opus); merged |
| SDK27 Mailgun | `work/w2-sdk27-mailgun`, `sdk50/w2-sdk27-mailgun` | Codex `gpt-6-astra` xhigh | complete: consumer (Opus), maintainer (Sol); merged |
| SDK28 Zendesk Support | `work/w2-sdk28-zendesk-support`, `sdk50/w2-sdk28-zendesk-support` | Claude subagent (Opus) xhigh | complete: consumer (Sol), maintainer (Terra); merged |
| SDK29 Intercom | `work/w2-sdk29-intercom`, `sdk50/w2-sdk29-intercom` | Codex `gpt-6.1-sol` xhigh | complete: consumer (Terra), maintainer (Astra); merged |
| SDK30 HubSpot CRM | `work/w2-sdk30-hubspot-crm`, `sdk50/w2-sdk30-hubspot-crm` | Codex `gpt-6.1-terra` xhigh | complete: consumer (Astra), maintainer (Opus); merged |

**Wave 2 closed** 2026-11-07 02:46 CET (last maintainer merged). 15 roles; observed settings in
`../../MODELS.md`. 33 findings in `FINDINGS.md`; synthesis and between-waves changes in
`SYNTHESIS.md` (lanes `sdk50/b23-core`, `sdk50/b23-oldadopt`, `sdk50/b23-w2adopt`, merged into the
integration at `4870f864`). Capsules and tickets: `../../roles/w2-SDK2x-*.md`.
