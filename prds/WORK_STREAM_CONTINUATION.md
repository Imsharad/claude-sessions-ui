# Work-stream continuation — product slice for Jules

Branch: `feat/work-stream-continuation`  
Spec: `design/resume-surface-spec-v2.md`  
Parent: `feat/review-tab` (report cards + ontology already present)

## Product problem

No canonical work streams surface for continuation. The user must search
hundreds of sessions or memorize what to resume. Home ranks by recency /
todos / pins only — it ignores digest `open_loops` and report-card
`desired_vs_real` open rows (stated unfinished intent).

## Ship this week (items 2+3 from the spec)

Feed digest open loops into the ranker and the hero card. Convert Home from
"most recent project" to "most important unfinished intent".

Do **not** implement snooze, WaitingShelf, or a new tab in this slice.

---

## Task A — Backend: L term + open loops + digest fields on HomeThread

**Files:** `src-tauri/src/home.rs`, `src-tauri/src/lib.rs` (`list_threads` only
if needed), mirror types if any.

### Requirements

1. Extend internal `SessionRow` / load path so the latest session can contribute:
   - `digest_open_loops: Vec<String>` from `session_digests.open_loops` (JSON
     array column; same parse pattern as `digest.rs` / `report.rs`)
   - `digest_worked_on: Option<String>`, `digest_outcome: Option<String>` from
     the same digest row for that session id (LEFT JOIN / correlated subquery)
   - `open_dvr_count: i64` — count of `desired_vs_real` rows with
     `"status":"open"` on the newest `project_reports` row matching the
     session's project key when practical; if joining by project key is hard,
     load open-loop strings from digests first and treat report open desired
     strings as additional open-loop sources for the latest project report
     whose digests touch that project. Prefer simple, correct SQL over clever.

2. Scoring change in `compute_metrics`:
   ```
   score = S * (100R + 40D + 30U + 15P + 25L)
   ```
   where `L = min(open_loop_count, 4) / 4.0` and
   `open_loop_count` = distinct open-loop strings from the **latest** session's
   digest `open_loops` plus open desired strings from report dvr (dedupe
   case-insensitively). Cap contribution at 4 items for the count.

3. `HomeThread` IPC fields (camelCase serde, matching existing style):
   - `open_loops: Vec<String>` — max 3 strings to show in the UI
   - `worked_on: Option<String>`
   - `outcome: Option<String>`
   Keep existing `open_todos`, `latest_recap`, etc.

4. Why-sentence: include an L fragment when L is among the top weighted
   components, e.g. `"2 open loops."` Prefer existing fragment style.

5. Unit tests in `home.rs` `mod tests`:
   - Thread with open digest loops ranks above an otherwise-equal thread without
   - `open_loops` on finalized thread capped at 3
   - Score formula includes 25L (spot-check with fixed rows)
   - Existing tests still pass

6. Acceptance: `cd src-tauri && cargo test home::` (or full `cargo test`) green.
   No new migrations. No snooze. No WaitingShelf.

---

## Task B — Frontend: hero open loops + digest-first body + Enter resume

**Files:** `src/lib/ipc.ts`, `src/components/HomeScreen.tsx` only.

### Requirements

1. Extend `HomeThread` in `ipc.ts`:
   ```ts
   openLoops: string[];
   workedOn: string | null;
   outcome: string | null;
   ```
   (Optional chaining / defaults if older backend: treat missing as `[]` / null.)

2. Hero body fallback chain:
   - Prefer `workedOn` / `outcome` (compose a short "Left off:" prose from them
     when present) over `latestRecap` over `latestTitle`.
   - Never invent text when digests are absent.

3. Open loops block (max 3):
   - Label: `Open loops` (or reuse "Left off" only for todos — prefer distinct
     `Open loops` for digest loops so intent vs mechanical todos stay clear).
   - If both `openLoops` and `openTodos` exist, show open loops first, then
     todos under a quieter "Todos" label.
   - Empty → render nothing (honest absence).

4. Keyboard (frontend only):
   - `Enter` resumes hero (`threads[0]`) via existing `resumeSession`, same
     double-fire guard as Resume button.
   - `2` / `3` resume secondary slots when present.
   - Ignore when focus is in an input/textarea/contenteditable.
   - Show a quiet `⏎` hint on the hero Resume button.

5. Design: no emojis; existing cream/ink tokens; no new libraries.

6. Acceptance: `npx tsc --noEmit` green. Do not break list/board/review views.

---

## Non-goals (both tasks)

- Snooze / `snoozed_until` migration
- WaitingShelf lane
- New HomeData shape beyond optional fields on HomeThread
- Changing cluster identity / ontology keys
- Notifications
- New LLM passes

## Homogeneous naming (do not break)

Display names stay as today unless already hub-prefixed elsewhere. Do not invent
a second work-stream identity system.
