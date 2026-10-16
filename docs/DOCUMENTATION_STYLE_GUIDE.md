# Documentation style

Date: 2026-10-16. Status: As-built conventions, derived from the existing decision records, interface reference, save-format contract and performance reports.

## Put each kind of information in its existing home

| Content | Home / example | Convention |
|---|---|---|
| Human onboarding | `README.md`, `CONTRIBUTING.md` | Short introduction, runnable setup, workflow and links to deeper references. |
| Product intent and phase scope | `PRD.md` | Cite the relevant section and distinguish planned scope from delivered behavior. |
| Architecture and owner decisions | `docs/decisions/0002-asset-pipeline.md` | Numbered filename and title, date and status near the top, rationale and evidence beside the decision. Preserve decision identifiers such as D2. |
| Agent-facing interfaces | `docs/agent-operability.md` | Tables of exact flags, defaults, effects and examples, followed by subsystem details. |
| Wire/data contracts | `docs/save-format.md` | As-built field layouts, units, validation order, version history and fixture paths. |
| Research and implementation plans | `docs/superpowers/research/`, `docs/superpowers/plans/` | Date-prefixed filenames and an opening scope/date. Historical recommendations are not current implementation claims. |
| Measurements | `docs/perf/stall-tail/README.md` | Record workload, platform, settings, load guard, results and what the evidence does or does not establish. |
| Visual review evidence | `docs/checkpoints/`, `docs/review/` | Link captures and distinguish pending review from an owner-recorded approval. |

## Describe what exists

Use present tense for shipped behavior and explicitly label planned, provisional, unverified or historical material. Decision records use a dated status such as `Accepted`; research briefs state their date and scope. Do not give every living reference a frozen status header just for uniformity.

Name the implementation or test that supports a technical claim. Existing gate documentation distinguishes checks pinned by a named test from checks that are **not pinned**. Keep that distinction. Write the constraint or reason, not a narrative of an agent's editing session.

Prefer tables for enumerable interfaces, thresholds, byte layouts and measured comparisons. Use prose for rationale and qualifications that would make a table cell unreadable. State units and exact defaults. Code fences hold commands that can be run from the stated directory. Quote identifiers and file paths with backticks.

## Keep references usable

Use repository-relative Markdown links for navigable documents. Cite source paths and symbol/test names beside claims, but do not depend on line numbers alone. Identify generated or deployed paths explicitly and give their source location, as with the HUD bundles and `web/ui/src/`.

A private vault or local-memory note is not a public repository link. Inline the relevant lesson or mark the citation as an **external vault note**. Do not imply that an external path exists in this checkout.

## Update with evidence

When behavior or commands change, update the corresponding living reference in the same change. Preserve dated research and measurement context rather than presenting its old recommendations as current facts. Verify paths, commands, defaults and table columns against the tree. For measurements, retain the actual workload and before/after load checks. A passing build alone does not prove prose is accurate.
