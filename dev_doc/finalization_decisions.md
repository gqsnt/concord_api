# v2.1 finalization decisions

The following decisions must be resolved from prototype evidence before release freeze.

| Decision | Initial target | Evidence | Finalization rule |
|---|---|---|---|
| representation trait/driver shape | generic directional pair with sealed core driver | Gate 1 passed mechanically | selected for prototype: exact pair traits plus a core-sealed acquisition driver |
| inherited representation | client/scope default + inline exception + fence | Gates 2–3 | retain only if effective origin is reliably discoverable and diagnostics local |
| optional `with_x` | retain during prototype | Gate 3 + large method count | keep if common-task gain is material and discovery/compile cost acceptable; otherwise remove before freeze |
| query words | `when_present`, `append`, `each` | Gate 3 | publish one spelling only |
| stream erasure | private boxed stream carriers | Gate 8 provisional | retain for prototype; compare enum/static/boxed before release freeze |
| observer events | attempt started, response head, operation finished, pagination finished, stream finished | Gates 5 and 9 passed mechanically; operator task review unavailable | retain minimum read-only set for prototype; no mutation authority |
| optional static secret | permit `secret x?` | auth construction task and security tests | keep or replace with explicit profiles; never silently omit placement |
| contract metadata size | static/effective runtime-accessible descriptions | Gate 10 provisional; 1–1,000 expansion evidence collected | retain for prototype; binary/memory and human first-screen evidence still gates freeze |
| aggregate body reuse | deferred | pagination/body benchmark | do not add unless explicit capability earns measured value |
| release version | 2.1.0 | all gates | bump only when active repository is one coherent target and migration path exists |

No finalization decision is made by retaining both alternatives as public aliases.
