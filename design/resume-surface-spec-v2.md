# Resume Surface v2 — design spec

Successor to `first-screen-spec.md`. That spec shipped as `home.rs` + `HomeScreen.tsx`
(hero + 2 secondary slots, `score = S * (100R + 40D + 30U + 15P)`). This spec is the
gap analysis and v2 design produced against the acceptance test:

> Within 5 seconds of launching the app, the user knows the ONE thing to resume,
> without reading more than ~3 items.

Core finding: the v1 ranker sees only session metadata (recency, density, todo
counts, pins). The app has since grown three richer intelligence layers the ranker
never consumes — digest `open_loops`, report-card `desired_vs_real` open rows, and
linked thread arcs. v2 is mostly about feeding those into the surface that already
exists, not building a new surface.

## 1. Ranking model

**Work stream** stays as defined in v1: the `home.rs` cluster — ontology key
(`ProjectIdentity.key`, hub/name collapse) with the 14-day branch split
(`{key} · {branch}` when a non-modal branch has >=2 recent sessions). This is the
right grain; do not change it.

**v2 score** extends v1 with one new term and one new lane:

```
score = S * (100R + 40D + 30U + 15P + 25L) * Z
```

- R, D, U, P, S — unchanged (recency 3-day half-life, density/14d, unfinished,
  pinned, substance guard).
- **L (open loops, 0..1)** — `min(open_loop_count, 4) / 4` where open_loop_count =
  distinct digest `open_loops` of the latest session plus report-card
  `desired_vs_real` rows with status "open" for the project. This is the
  highest-value signal the ranker currently discards: it measures *stated unfinished
  intent*, not just mechanical todo counts. U catches "left mid-task"; L catches
  "left with known debts".
- **Z (snooze, 0 or 1)** — 0 while `snoozed_until > now` on the thread key.
  User-set, one interaction (see section 5).

**Contradictory signals, resolved explicitly:**
- Project status archived/rejected (override-wins, already loaded) → excluded
  entirely. Status beats every positive signal including pins.
- Pinned but recency-dead (R < 0.05, i.e. untouched ~13+ days) → does NOT compete
  for the hero. It moves to the "waiting" shelf (section 4). A pin means "don't
  let me forget", not "this is what I'm doing today".
- Snoozed but pinned → snooze wins (it's the more recent instruction).
- Ties → later `last_ts`, then key, matching v1's deterministic ordering.

## 2. Placement

**Primary: launch screen (the existing Home view).** Already correct — it is the
first pixels on open and the ranking is deliberately stable per app-open (App.tsx
does not re-fetch on view switches), which protects the 5-second test from
mid-glance reshuffles. Keep that invariant.

Rejected alternatives: persistent tab (already have it — Home *is* a tab; the point
is it's also the landing), command palette (recall-oriented; resumption is
recognition-oriented — a palette requires the user to already know what to type),
ambient menubar/notifications (interruption is the opposite of an ADHD-safe design;
the app should answer "what was I doing" when asked, never tap the shoulder).

One addition: **Enter resumes the hero.** App open → read hero → Enter → terminal
opens with `claude --resume`. That is the 5-second test made literal: zero mouse,
one key.

## 3. Three design directions

**A. Next-action stack (v1 extended).** Hero card answers "resume what, and why";
below it two secondary rows, then browse-all.
- Core interaction: Enter resumes hero; 2/3 resume secondaries.
- Kill risk: hero trust. One wrong hero and the user starts scanning everything,
  which collapses into direction C's failure. Mitigated by the L term + snooze.

**B. Narrative re-entry ("while you were away").** A digest-written paragraph:
"Yesterday you landed the report pipeline; you left three loops open: …" with
inline resume links. Time-anchored, reads like a standup note.
- Core interaction: read, then click a loop to resume into it.
- Kill risk: reading cost. Prose must be read fully before acting — directly
  violates the 5-second test on busy days. Also fully hostage to digest coverage;
  a `skipped_no_recap` session makes the narrative silently wrong.

**C. Spatial project map.** Hub-grouped board (NOW / personal-hq / labs), streams
as cards colored by lifecycle, size by activity.
- Core interaction: glance at the "hot" region, click a card.
- Kill risk: it's an overview, not an answer. Presents 20 options with equal
  weight and makes the user do the ranking — the exact cognitive load this
  surface exists to remove. (KanbanBoard already covers the deliberate-browse
  version of this need.)

## 4. Recommendation: A, with B's content inside the hero

Keep the stack. Upgrade what the hero *says* using the narrative layer's data,
without making prose the navigation:

```
 ┌────────────────────────────────────────────────────────────┐
 │ NOW / claude-sessions-ui · feat/review-tab        2h ago   │
 │                                                            │
 │ Left off: report-card pipeline landed; ReviewView          │
 │ three-tier UI in place.                        [digest]    │
 │                                                            │
 │ Open loops                                                 │
 │   - e2e capture flow unverified on fresh index             │
 │   - REVIEW.md screenshots not wired into CI                │
 │                                                            │
 │ Why: active 6 of last 14 days; 2 open loops.               │
 │                                                            │
 │ [ Resume  ⏎ ]   [ Fork ]   [ Snooze ▾ ]                    │
 ├────────────────────────────────────────────────────────────┤
 │ 2  brain · distill hardening        1d ago   why…  [Resume]│
 │ 3  sharadja.in · blog post          2d ago   why…  [Resume]│
 ├────────────────────────────────────────────────────────────┤
 │ Waiting on you (2)                              collapsed ▸│
 └────────────────────────────────────────────────────────────┘
```

**Component hierarchy** (deltas to `HomeScreen.tsx` only):
- `HeroCard` — swap `latest_recap` body for digest `worked_on`/`outcome` when a
  digest exists (fall back to recap, then title); add `OpenLoops` list (max 3
  shown, "+N more"); add `SnoozeMenu` (rest of day / 3 days / until I touch it);
  keep existing Resume/Fork buttons; register global Enter → hero resume
  (disabled while any resume is in flight — the existing double-fire guard).
- `SecondaryRow` — unchanged except keybindings 2/3.
- `WaitingShelf` — new, collapsed by default, fed by the pinned-but-decayed and
  lifecycle-active-but-decayed lane from section 1. Renders like browse rows
  with the why-sentence "Pinned, untouched 3 weeks."
- Browse list, stale mode, FirstRun — unchanged.

**States:** loading — v1 skeleton unchanged. Empty — FirstRun unchanged. Digest
missing for hero — recap fallback, no loops block, no "[digest]" chip (never fake
the intelligence layer; honest-absence is already this codebase's idiom).

**The resume action:** Enter/click fires the existing `resume_session(id, fork)`
→ terminal at the session's cwd with `--resume`. No new semantics.

## 5. Edge cases

- **Cold start** — FirstRun scan flow already handles it. After first index with
  zero digests, the surface is v1 exactly; intelligence appears as digests backfill.
- **30+ active streams** — grain (ontology collapse + branch-split) plus the
  2-slot dominance guard already bound the visible set at 3 + shelf + browse.
  Overload lands in the browse list, which is paginated reading, not a decision.
- **Stale-but-important** — the WaitingShelf lane: pinned or lifecycle-active
  threads with R < 0.05. They stop competing for the hero (they'd always lose on
  recency and silently vanish in v1 — the current design's real gap) and instead
  get a persistent, low-pressure home that does not decay away.
- **Wrong ranking, one-interaction recovery** — two directions: promote = pin
  (exists, `toggle_pin`); demote = snooze (new). Snooze writes `snoozed_until`
  on the thread key and Z zeroes the score until expiry or until a new session
  touches the thread (any new activity clears it — activity is the strongest
  "not snoozed anymore" evidence).

## 6. Implementation map

Ordered by effort, each independently shippable:

1. **Enter-to-resume + 2/3 keys** — frontend only (`HomeScreen.tsx` keydown,
   reuse the in-flight guard). Hours.
2. **L term + loops in hero** — `load_session_rows` gains two correlated
   subqueries (digests.open_loops for the latest session; count of open
   desired_vs_real rows from reports by project key — both tables exist);
   `compute_metrics` adds the term; `HomeThread` gains `openLoops: Vec<String>`.
   Unit-testable in the existing pure-function harness. Small.
3. **Digest-first hero body** — `worked_on`/`outcome` already reachable via the
   same join; frontend fallback chain. Small, pairs with 2.
4. **Snooze** — migration (v7 slot): `snoozed_until` on projects keyed like pins;
   `set_snooze` command mirroring `toggle_pin`; Z factor + clear-on-activity in
   `build_home`; menu in hero/secondary. Medium-small.
5. **WaitingShelf** — pure ranking change in `build_home` (emit a second lane in
   `HomeData`) + collapsed section in HomeScreen. Medium.
6. **Cut: time-of-day/day-of-week patterns** — needs weeks of behavioral data to
   beat "most recent unfinished thing", and a wrong circadian guess costs hero
   trust, the one thing the surface cannot spend. Revisit only if the hero is
   measurably wrong at specific hours.
7. **Cut for now: "since you left" git delta** — per-cwd `git status` at launch
   is a latency and permission surface; the digest outcome line covers 80% of
   the value. Candidate for async fill-in later.

## If you build only one thing this week

Ship items 2+3 together: feed digest open loops into the ranker and the hero card.
It is the single change that converts the home screen from "most recent project"
to "most important unfinished intent" — the actual ask — and it is small because
both the data (digests table) and the surface (hero card) already exist; only the
join between them is missing.
