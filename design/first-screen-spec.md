# First-Screen Spec — claude-sessions-ui

Filled from the live codebase:

- **APP_NAME**: claude-sessions-ui (Tauri desktop app indexing local Claude Code sessions; ~688 indexed)
- **DATA_FIELDS**: `SessionCard` (src/lib/ipc.ts:47) — `id, projectDir, cwd, displayProject, gitBranch, title, firstTs, lastTs, messageCount, durationMs, planMode, hasRecap, recap, inputToks, outputToks, costUsd, pinned, areaOfLife, projectShortName, goalCompleted, completionPct, tagRationale, kanbanStatus, kanbanOrder` — plus per-session SQLite tables already indexed: `todos(seq, content, status)`, `recaps(content, seq, is_final)`, `errors`, `files_touched`, `turns`.
- **CURRENT_FIRST_SCREEN**: TopBar (view tabs, stats, reindex) + Sidebar (project list, pinned, search, blacklist) + virtualized flat list of ALL sessions ordered `pinned DESC, last_ts DESC` + permanently mounted 420px SessionDetail pane.
- **MAX_THREADS**: 5
- **CONSTRAINTS**: one-click resume already exists (`resume_session(id, fork)`); tags are partially populated (triage is manual, AI tagging parked), so every signal that depends on tags must degrade gracefully to untagged data.

North star: the user opens the app asking "what was I working on recently, and where do I pick it back up?" Everything below serves only that question.

---

## 1. INFORMATION HIERARCHY

Top to bottom. Space and detail decay with rank.

**Slot 0 — Orientation line (one line, no chrome).**
"Tuesday, July 5 — 3 threads active this week." Earned its slot because a resume decision needs a time anchor before anything else; it is one line and never grows.

**Slot 1 — HERO thread (~45% of viewport).**
The single most likely "resume this" answer. Shows: thread name (`projectShortName` else `displayProject`) + `areaOfLife` chip + `gitBranch`; the latest session's `recap` at 3–4 lines (not truncated to a headline); a "Left off" block — up to 3 `todos` from the latest session with `status != "completed"`, or if none, the last non-final recap line; the one-sentence why ("Active 3 of the last 4 days, last session ended with 2 open todos"); a primary **Resume** button wired to `resume_session(latestSessionId)` with a quiet fork affordance beside it; relative `lastTs`. Earned the slot because it is the answer to the north-star question — everything else on screen is a fallback in case the ranking guessed wrong.

**Slots 2–3 — Secondary threads (one row each, ~2 lines).**
Thread name + chip, one-line latest recap (truncated 120), the one-sentence why, relative time, a compact Resume button. Earned their slots because the ranking is a heuristic: the true answer is in the top 3 far more often than it is exactly rank 1.

**Slots 4–5 — Tertiary threads (one thin line each).**
Name, relative time, recap truncated to 60 chars. No why-sentence, no button (click opens the thread, resume is one more click). Earned their slots as peripheral-vision recall — legible in a glance, ignorable otherwise.

**Slot 6 — The exit (single subordinate row, bottom).**
"Browse all 688 sessions" + the search field. Visible, visually quiet (ink-3, no border weight). Everything the current first screen does — flat list, sidebar, board, triage, detail pane — lives exactly one interaction behind this row. Earned its slot because hard rule 4 requires the escape hatch to be visible; it earns nothing more than one row.

Nothing else appears. No stats, no costs, no token counts, no reindex button (incremental scan already runs silently on launch; surface a toast only on scan error).

---

## 2. RANKING & CLUSTERING LOGIC

Implementable as one new IPC command `list_threads(limit: 5)` doing SQL over the existing tables. No new data collection required.

### Clustering rule (sessions → threads)

1. `thread_key = projectShortName ?? displayProject`. Tagged and untagged sessions of the same project merge only when `projectShortName` matches `displayProject` case-insensitively; otherwise the tag wins for tagged rows and untagged rows keep `displayProject` (they merge later as triage catches up — acceptable, self-healing).
2. **Branch split**: within a thread, if ≥2 sessions in the last 14 days share a `gitBranch` that differs from the thread's modal branch, those sessions split into a sub-thread keyed `thread_key + "·" + gitBranch`. One-off branch sessions do not split.
3. Blacklisted projects are already excluded by `list_sessions`; reuse the same filter.

### Scoring heuristic (per thread)

Let `d` = days since the thread's max `lastTs`, computed at query time.

- **Recency** `R = 2^(-d/3)` (3-day half-life; a thread untouched for 9 days keeps ~12% of its recency weight).
- **Density** `D = distinct active days in the last 14 / 14`, where an active day is any day containing a session of the thread.
- **Unfinished** `U = 1` if ANY of, on the thread's latest session: a `todos` row with `status != "completed"`; `completionPct` set and `< 100`; `goalCompleted = false`; `kanbanStatus = "in_progress"`; or `planMode = true` with no later session in the thread (a plan written and never executed). Else `U = 0`.
- **Pinned** `P = 1` if the project row is pinned.
- **Substance guard** `S = 0` if the thread has exactly 1 session AND `messageCount < 10` AND `U = 0`; else `S = 1`. (Kills drive-by one-offs.)

`score = S * (100*R + 40*D + 30*U + 15*P)`

Rationale for the weights: recency dominates (this is a "recently" question), density breaks ties between two recently-touched threads in favor of the one that is a sustained line of effort, unfinishedness outranks pinning because "left mid-task" is a stronger resume signal than a manual bookmark, and the substance guard means no weight juggling can surface noise.

**Dominance guard**: at most 2 threads sharing the same project `thread_key` prefix (i.e., a project and one of its branch sub-threads) may appear in the 5 slots; further sub-threads of that project are folded back into its top thread and the freed slots go to the next distinct projects.

### Explainability (hard rule 6)

The why-sentence is generated from the two largest score components, from templates — no free text:

- R dominant: "Last active {relativeTime}."
- D dominant: "Active {activeDays} of the last 14 days."
- U dominant: "Last session ended with {n} open todos." / "…at {completionPct}% complete." / "…with an unexecuted plan."
- P dominant: "Pinned."

Joined with "; " for the top two components. Every surfaced thread can state its reason in one sentence by construction.

---

## 3. WIREFRAME

```
+----------------------------------------------------------------------+
| Tuesday, July 5 — 3 threads active this week                         |
+----------------------------------------------------------------------+
|                                                                      |
|  claude-sessions-ui   [Building]  main             2h ago            |
|  ------------------------------------------------------------------ |
|  Recap: Shipped honest two-source blacklist match counts and the    |
|  header-first panel; board keeps three columns co-visible with      |
|  the detail as an overlay. Started wiring the triage queue to...    |
|                                                                      |
|  Left off:                                                           |
|   [ ] wire triage exit refresh to the board column derivation       |
|   [ ] screenshot pass on hidden-10x captures                        |
|                                                                      |
|  Active 3 of the last 4 days; last session ended with 2 open todos. |
|                                                                      |
|  [ Resume ]  (fork)                                    ~45% height   |
+----------------------------------------------------------------------+
|  brain · sleep-consolidation  [Research]            yesterday        |
|  "P1 audit for Chrome ETL passing; bookmarks rollup still stubbed"  |
|  Active 5 of the last 14 days.                          [ Resume ]  |
+----------------------------------------------------------------------+
|  udacity-reviews-hq  [Ops]                          2 days ago       |
|  "Rubric-aligned feedback generator; batch 3 graded, batch 4 mid"   |
|  Last session at 60% complete.                          [ Resume ]  |
+----------------------------------------------------------------------+
|  sharadja.in — "orgs-as-code draft, section 2"       4 days ago      |
|  little_bird_app — "EventBus refactor, tests green"  6 days ago      |
+----------------------------------------------------------------------+
|  Browse all 688 sessions                                 [search __] |
+----------------------------------------------------------------------+
```

Space allocation: orientation ~4%, hero ~45%, slots 2–3 ~15% each, slots 4–5 ~5% each, exit row ~5%, whitespace the rest. The hero's Resume button is the largest interactive target on screen.

---

## 4. EDGE-CASE BEHAVIOR

**Fresh install / no history.** The existing `FirstRun` scan runs; if it completes with `sessionCount = 0`, the first screen shows a single hero-sized card: "No Claude Code sessions found yet. Sessions appear here after you run `claude` in any project." No empty slots, no exit row (nothing to browse).

**Returning after a long gap.** If the global max `lastTs` is older than 14 days, `R` is near zero for everyone, so: compute `D` over the 14-day window ending at max `lastTs` instead of now, and swap the hero framing from "resume" to memory-jog copy — orientation line becomes "You were last here June 12"; the hero why-sentence becomes "When you left, this was active 4 of your last 14 days, with 2 open todos." Resume button unchanged — the answer is stale but still the answer.

**One thread dominates all activity.** The dominance guard caps any project at 2 of the 5 slots. If fewer than 5 distinct qualifying threads exist, render fewer slots — empty space beats padding with noise (rule: never backfill with sub-threshold threads just to fill the budget).

**Many tiny one-off sessions.** The substance guard (`S = 0`) removes them from ranking entirely; they are reachable only via Browse all. If tiny one-offs are ALL that exists (new user exploring), relax the guard and rank whatever exists by `R` alone — a weak answer beats a blank screen, and the why-sentence honestly says "Last active {time}."

---

## 5. WHAT WAS REMOVED

| Current element | Disposition |
|---|---|
| Flat virtualized list of all 688 sessions | Demoted behind "Browse all" — becomes the browse view, unchanged. The dump is the archive, not the front door. |
| Sidebar (project list, pinned toggles, blacklist manager) | Moved into the browse view. Project navigation is a browsing task, not a resume task. Pinning still feeds ranking (P term) so it keeps its effect without its pixels. |
| Always-mounted 420px SessionDetail pane | Browse view only, on click. On the first screen the hero card IS the detail for the one session that matters. |
| TopBar stats (session count, cost, tokens) | Behind Browse all / future Analytics view. Vanity metrics answer no resume question. |
| Reindex button + index status | Removed from first screen; incremental scan already runs on launch (App.tsx bootstrap). Surface a toast only on `ScanStats.error`. |
| Launcher mode toggles (list / board / triage) | Moved into the browse view header. Board and triage are curation tools; curation output (kanbanStatus, tags) feeds ranking instead. |
| Per-card cost/token/duration metadata | Detail tiers in browse view. None of it changes what you resume. |
| Search field | KEPT, demoted to the exit row — hard rule 4 requires a visible but subordinate escape hatch, and search is the fastest correction when all 5 slots are wrong. |
| Recap-led card headline + area chip + completion badge | KEPT as the vocabulary of the thread rows — this part of the current screen already serves the north star; it was ranked wrong, not designed wrong. |
| Resume action (`resume_session`) | KEPT and promoted from a detail-pane action to the primary button on screen. |

---

## Self-check against hard rules

1. No flat list first — the dump is one interaction away. PASS.
2. Unit of display is the thread (project / project·branch cluster), never a raw session. PASS.
3. Zero-interaction test — hero shows what/where-left-off/why/resume above the fold with no input. PASS.
4. Max 5 threads + one visible subordinate exit. PASS.
5. Detail decays monotonically: 4-line recap + todos + button → 1-line + button → name + fragment. PASS.
6. Why-sentence generated from score components by template for every surfaced thread. PASS.

## Implementation seam (for the next episode)

One new Rust command `list_threads` (SQL: group `sessions` by resolved thread key, join `todos` for open-todo counts on each thread's latest session, compute R/D/U/P/S in Rust, return `ThreadCard[]`), one new `HomeScreen.tsx` rendering slots 0–6, and `App.tsx` gains `view: "home"` as the default with the existing launcher demoted to the browse view. No schema changes; every signal reads tables the indexer already populates.
