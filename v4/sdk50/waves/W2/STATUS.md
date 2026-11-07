# Wave 2 status

Frozen library baseline: `8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f` (`sdk50/integration`): Wave 1
SDKs, the between-waves changes and both reviews' fixes. Gate on it: `scripts/verify.sh --package
--bench` 66/66 PASS, clean, 6m52s; `--nightly-only` PASS (16 tests). Authors started 2026-11-07
02:00 CET.

| SDK | Worktree / branch | Author (requested) | Status |
|---|---|---|---|
| SDK26 Postmark | `work/w2-sdk26-postmark`, `sdk50/w2-sdk26-postmark` | Codex `gpt-6.1-terra` xhigh | authoring |
| SDK27 Mailgun | `work/w2-sdk27-mailgun`, `sdk50/w2-sdk27-mailgun` | Codex `gpt-6-astra` xhigh | authoring |
| SDK28 Zendesk Support | `work/w2-sdk28-zendesk-support`, `sdk50/w2-sdk28-zendesk-support` | Claude subagent (Opus) xhigh | authoring |
| SDK29 Intercom | `work/w2-sdk29-intercom`, `sdk50/w2-sdk29-intercom` | Codex `gpt-6.1-sol` xhigh | authoring |
| SDK30 HubSpot CRM | `work/w2-sdk30-hubspot-crm`, `sdk50/w2-sdk30-hubspot-crm` | Codex `gpt-6.1-terra` xhigh | authoring |

Observed settings are recorded in `../../MODELS.md` when each role ends. Capsules:
`../../roles/w2-SDK2x-author.md` (copied when the wave closes).
