# Build Prompt — "Review" Tab (per-project report cards)

*Iterated from seed: "insights tab mapping temporally aware recaps to
ontologically derived project names, per-project report card for last N days —
what was built, how, and why — connecting desired to real artifacts in
minimalist progressively disclosed cards."*

---

## The one-sentence job

Replace the `digest` ComingSoon stub with a **Review** tab: for a chosen
window (last 7 / 14 / 30 days), group every digested session under its
ontology-derived project and render one report card per project answering
three questions — **what was built, how it was built, and why** — where every
claim is linked to real evidence (sessions, files, citations), disclosed
progressively from a one-line glance down to the underlying session.

Name decision: the tab is called **Review** (not "Insights"). It matches the
existing mockup (`design/mockups/review-ontology.html`), reads as a weekly-
review ritual rather than a dashboard, and stays distinct from the Analytics
tab (numbers) — Review is narrative, Analytics is metrics.

## Why this feature exists

The app already answers "what happened in one session" (recap) and "what
happened this day" (timeline). Nothing answers "what happened to this
*project* lately, and did the work I intended actually land?" That is the
question Sharad opens the app with on Monday morning. The report card is the
desired-vs-real reconciliation: intent (worked_on, open_loops) on one side,
artifacts (outcomes, files, citations) on the other.

## Data reality (build on this, do not reinvent)

All of this exists in `src-tauri/src`:

- `session_digests` — per-session LLM recap: `worked_on`, `outcome`,
  `open_loops` (JSON, max 3), `citations` (JSON), `verified`, `confidence`.
  Schema-validated in Rust; `content_hash` (FNV-1a) gates regeneration;
  `manual_fields` protects hand edits. **Reuse this exact guardrail pattern
  for report-card generation.**
- `threads` + `thread_members` — narrative arcs already linking sessions
  across days. A report card's "how" section should surface arcs, not
  recompute them.
- `projects` table — `encoded_dir`, `cwd`, `display_name`, `pinned`.
- `sessions` — `cwd`, `project_dir`, `first_ts`, `last_ts`; blacklist
  filtering via `dir_blacklisted` (respect it everywhere).
- `files_touched`, `session_usage` (cost/tokens), `errors`, `todos`.
- `digest.rs` pipeline: `build_timeline`, `digest_pending_blocking`,
  `link_threads` — the report-card pass is a fourth sibling pass, same shape.
- Project identity today is only `project_tail(cwd)` (last path segment,
  `digest.rs`). There is **no ontology code in the codebase** — the
  `review-ontology.html` mockup is an unmerged proposal. The ontology
  derivation below is NEW scope this feature lands.

## Backend deliverable

A small ontology module, one new pipeline pass, and two tauri commands,
mirroring the digest pass:

0. **Ontology module (NEW, deterministic Rust, no LLM).** `derive_project_key(cwd)
   -> (Option<hub>, name)`: if cwd is under `~/Projects/<hub>/<name>/...` with
   hub in {`NOW`, `agents-hq`, `personal-hq`, `labs`, `_archive`}, identity is
   `(hub, name)` — so any subdir or in-repo worktree of the same project
   collapses to one key. Any other path falls back to `(None,
   project_tail(cwd))`. Pure function over the path string; no new tables or
   columns (lifecycle status from the ontology mockup is explicitly out of
   scope). Known v1 limit: worktrees living outside the project dir (e.g.
   `.conductor` checkouts elsewhere) do not collapse — acceptable, note it in
   code.
1. **Grouping (deterministic Rust, no LLM).** For the window, collect
   non-blacklisted sessions having a digest row; group by
   `derive_project_key(cwd)`. Sessions without digests are counted but excluded from prose
   ("3 sessions not yet digested" is shown honestly on the card, reusing the
   `no_recap` TagError vocabulary — never silently dropped).
2. **Report-card generation (LLM, guarded).** Per project-window, one prompt
   whose input is that project's digest rows + thread arcs + files/cost
   aggregates, and whose output is schema-validated JSON:
   - `built` — max 4 bullets, each `{claim, evidence: [session_id, ...]}`.
     A bullet with zero resolvable evidence ids is rejected in Rust
     (referential check against the input set, same as thread linking).
   - `how` — max 3 bullets: approach/method, tied to thread arcs where they exist.
   - `why` — max 2 bullets: the intent, sourced from worked_on/open_loops.
   - `desired_vs_real` — max 3 rows `{desired, real, status: landed|partial|open}`.
     This is the seed's "connecting desired to real artifacts" made literal:
     desired comes from open_loops/todos, real from outcomes/files_touched.
   - `headline` — one calm sentence, ≤ 120 chars.
   Persist in a `project_reports` table keyed `(project_key, window_days,
   window_end_date)` with `content_hash` over input digest hashes +
   prompt_version + model, so an unchanged week never regenerates.
3. **Commands:** `get_review(days)` (grouped cards, cache-first) and
   `generate_reports(days)` (runs pending generation, returns batch report
   like `digest_pending`).

## Frontend deliverable

`ReviewView.tsx` wired into the existing `view === "digest"` slot (rename the
View union member to `review`). Three disclosure tiers — each tier answers a
question, and nothing from a deeper tier leaks upward:

- **Tier 0 — the shelf (default).** One row-card per project, sorted by
  recency-weighted activity. Contents only: project name, hub as a quiet
  prefix (`NOW /`), headline sentence, and a right-aligned quiet stat cluster
  (sessions · files · cost, dot-separated per FRONTEND.md metadata rule).
  Whole window scannable in one pass with zero scrolling for ≤ 8 projects.
- **Tier 1 — the card, expanded (click).** In-place expansion (spring, no
  route change): the three labeled sections (Built / How / Why) as short
  bullet lists, then the desired-vs-real rows with status glyphs (text
  markers, no emojis; semantic green/amber only, per pillar 3). Each bullet's
  evidence renders as small session chips.
- **Tier 2 — evidence (click a chip).** Opens the existing `SessionDetail`
  for that session. No new detail surface — reuse.

Window control: a single segmented `7d / 14d / 30d` in the view header,
default 7. Regeneration affordance: one quiet "refresh" action per card and
one per view; show `generated_at` age subtly ("as of yesterday").

## Design constraints (non-negotiable, from FRONTEND.md)

Creamy, calm, confident — warm paper canvas, cream surfaces, warm ink. One
soft-blue accent for selection only. 12–16px radii, layered shadows, spring
motion. Honest density: real numbers, grouped for one-pass scanning. The
headline sentence is the hero of each card, exactly as recap is the hero of a
session card. No emojis anywhere. If a card reads like a gray SaaS dashboard
widget, it failed.

## Honesty rules

- Every "built" claim carries clickable evidence or it does not render.
- Low-confidence or unverified digests: aggregate normally but style the
  derived bullets with the existing unverified treatment.
- Empty window for a project → project simply absent (no ghost cards).
- No digested sessions at all → empty state that offers to run
  `generate_reports`, not a lecture.
- Manual edits: `manual_fields` semantics apply to report cards too — a
  hand-edited headline survives regeneration.

## Non-goals (v1)

No cross-project synthesis paragraph, no charts (Analytics owns numbers), no
date-range picker beyond the three presets, no editing of Built/How/Why
bullets in-app beyond the headline, no export. No project lifecycle status
(`projects.status` columns, archived filtering, Home/Launcher/Timeline
changes) — that is the separate unmerged Project Ontology Phase 1 diff; this
feature only lands the identity parser it needs.

## Acceptance

- `cargo test` green, including: `derive_project_key` unit tests (hub paths,
  subdirs and in-repo worktrees collapse; non-hub and non-`~/Projects` paths
  fall back to tail; `_archive` recognized); report rejected when a claim
  cites an unknown session id;
  content_hash stability across identical windows; blacklisted dirs never
  appear.
- Wire format proven serde-camelCase like existing commands.
- `bun run build` + tsc clean.
- Visual QA via the vision-qa-loop skill against the real app: capture
  tier 0/1/2, verify warmth, hierarchy (headline > sections > evidence), and
  the one-pass scan rule.
