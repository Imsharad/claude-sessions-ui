# Tag and Triage — Master Build Prompt (v3, repo-grounded)

*Paste as the first message of a fresh Claude Code session in this repo. The
plan: turn the sessions browser from a passive list into a tag-and-triage
board — curate what gets tracked (blacklist), glance without clicking
(progressive cards), classify in one shot (AI tagging), track to done
(kanban). Every claim below is verified against the current codebase. Read it
top to bottom before writing code.*

---

## Role

You are a senior engineer **and** the taste owner for this tool. Two bars, both
mandatory: it must **work end-to-end**, and it must **feel** like the north-star
in `FRONTEND.md` — "creamy, calm, confident," Mail.app/Linear gene, not a gray
SaaS dashboard. A feature that functions but reads as a generic admin panel is a
failed feature. Ship taste, not just tickets.

## The repo, precisely

Tauri v2. Rust backend in `src-tauri/src/{lib.rs,db.rs,claude.rs,indexer.rs}`,
SQLite via `db.rs`. Frontend: React 19 + Vite + Tailwind **v4** + TypeScript.
IPC in `src/lib/ipc.ts`; components in `src/components/` (`SessionList`,
`SessionDetail`, `Sidebar`, `TopBar`, `FirstRun`, `ErrorBoundary`). Deps
already present — extend these, add **no** new framework: `cmdk`,
`framer-motion`, `recharts`, `@tanstack/react-virtual`, `lucide-react`,
`date-fns`.

Sessions are indexed from Claude Code logs; each may carry a **recap** (the
auto-generated goal+summary — the product's hero object).

**Before coding, read:** `FRONTEND.md` (design law), `src-tauri/src/lib.rs`
(the `#[tauri::command]` surface + serde structs), `src-tauri/src/db.rs`
(schema + `migrate()`), `src-tauri/src/indexer.rs` (the scan/skip loop),
`src/lib/ipc.ts` (wire format), `src/components/SessionList.tsx` and
`SessionDetail.tsx`. Match their patterns exactly; do not reinvent.

---

## Design DNA (non-negotiable — from FRONTEND.md)

Use the **existing tokens**, never raw hex or a new scale:

- Surfaces: `bg-canvas` (warm paper), `bg-surface`, `surface-3`. Never pure
  white, never gray. If it reads gray, it failed pillar 1.
- Ink: `text-ink`, `text-ink-2`, `text-ink-3`, `text-ink-4` (warm dark →
  muted). Never pure black or blue-gray.
- Accent: `accent`, `accent-soft`, `accent-strong` — **one** soft blue, for
  selection/active/links only. Never decorative. Introducing a second accent is
  a rejection-level error.
- Semantic color (green/amber/red) only for status, never for chrome.
- Radius `rounded-lg` (12–16px family), layered soft shadows, gentle hover lift
  (`whileHover={{ y: -1 }}`). All motion is **spring physics** via framer
  (`type: "spring", stiffness: 400, damping: 30`), never linear/ease.
- **Recap is the hero.** It is the headline of every card and the centerpiece of
  detail. Title is a quiet subtitle. A `Sparkles` glyph marks recap-bearing
  sessions. New fields you add (area, completion, tags) are **subordinate** to
  the recap — they garnish, they do not compete.
- Honest density: real metadata, grouped so the eye scans in one pass
  (`project · branch | time · msgs · duration | cost · tokens`). New chips join a
  group; they do not start a new noisy row.

**Taste gate for every UI change:** "This serves pillar N. It makes the app feel
warmer/calmer/more scannable. It adds no visual noise, hardens no edges,
introduces no second accent. If a screenshot reads more gray/busy/cramped after
this change, revert."

---

## Reality checks (five landmines — honor these or the build breaks)

1. **There is no LLM client.** `claude.rs` is an `osascript` Terminal launcher
   (`claude --resume <id>`), not an API client — no HTTP, no API key, no SDK.
   For Feature 3 do **not** invent an HTTP client or bolt in `ANTHROPIC_API_KEY`.
   Reuse the app's existing philosophy: **shell out to the local `claude` CLI in
   headless mode** — `claude -p "<prompt>" --output-format json --model
   claude-haiku-4-5` — parse stdout, extract the JSON object. This inherits the
   user's existing auth, needs zero key management, and matches `claude.rs`'s
   `std::process::Command` pattern. Put it in `claude.rs` beside
   `open_in_terminal`. Make the command `async` (tauri commands support async);
   enforce a bounded timeout; non-zero exit → typed error surfaced to the UI,
   never a silent hang. If you believe HTTP is warranted instead, **stop and
   ask** — do not add a key-management surface unprompted.

2. **A GUI process does not inherit your shell PATH.** The osascript launcher
   dodges this (it runs inside Terminal); a headless `Command::new("claude")`
   does not. Resolve the binary once at first use — `/bin/zsh -lc "command -v
   claude"` or a fallback list of known install paths — cache the result, and
   surface a typed "claude CLI not found" error the UI can show with guidance.
   Never assume bare `claude` resolves.

3. **There is no migration framework.** `migrate()` is a single `execute_batch`
   of `CREATE TABLE IF NOT EXISTS` plus `seed_pricing_if_empty`; there is no
   `PRAGMA user_version`. Establish the pattern in your first migration:
   - Read `PRAGMA user_version`; run additive steps guarded by it, then bump it.
   - New tables: `CREATE TABLE IF NOT EXISTS` (safe, idempotent).
   - New **columns** on `sessions`: SQLite `ALTER TABLE ADD COLUMN` has **no**
     `IF NOT EXISTS` — guard with a `PRAGMA table_info(sessions)` column-presence
     check before altering, so re-running is idempotent. Keep field names stable
     (indexer + frontend depend on them).

4. **The list is fixed-height virtualized.** `SessionList.tsx` uses
   `ROW_HEIGHT = 116` with `@tanstack/react-virtual`. Feature 2's expand-in-place
   changes a row's height, which fights fixed sizing. You must drive height from
   `virtualizer.measureElement` (already wired on the row `ref`) and animate with
   framer `layout` such that the virtualizer re-measures on expand/collapse. Test
   with the full dataset (~700 rows): scrolling must stay at 60fps and expanded
   rows must not overlap neighbors. Do not swap the virtualization library.

5. **The wire format is camelCase serde.** Every Rust struct carries
   `#[serde(rename_all = "camelCase")]`; `ipc.ts` mirrors field names exactly
   (`project_dir` ↔ `projectDir`). Every new backend capability = a
   `#[tauri::command]` + an entry in `generate_handler![]` + a typed wrapper in
   `ipc.ts` + a matching TS interface. No exceptions.

---

## Cross-feature data rule

Feature 2's Tier-1 chips (area-of-life, completion %) read columns that only
Feature 3 populates. Ship the **full schema in the first migration**, and make
every chip **gracefully absent**: a session with no area/completion renders a
clean card with no placeholder, no empty badge, no "untagged" label. Absence is
the default state of the whole dataset on day one — it must look intentional,
not broken.

## Verification protocol (after every feature, before its commit)

1. `cargo test` in `src-tauri/` — all existing characterization tests stay
   green; add coverage for new pure logic (glob matching, JSON validation,
   column derivation).
2. `bun run build` — `tsc` + vite must pass clean.
3. `bun run test:e2e` — the WebDriver golden-master specs (7 green today) stay
   green; update a golden only when the feature intentionally changed that
   surface, and say so in the commit message. Add a spec when a feature adds a
   primary flow (blacklist toggle, card expand, tag action, board drag).
4. Run the app and exercise the feature end-to-end, then apply the taste gate.
   "It compiles" is not verification.

**Blocked protocol:** if this prompt contradicts what you find in the repo,
trust the repo, note the discrepancy in one line, and proceed with the smallest
interpretation consistent with the code. Stop and ask only for: the HTTP
question in landmine 1, adding any new dependency, or destructive schema
changes.

## Deliverable order

State your **migration/data plan first** (tables, columns, `user_version`
steps, new commands + their ipc wrappers) — covering all four features so the
schema lands once. Then implement features **1 → 2 → 3 → 4**, each
independently working, verified per the protocol above, and **committed
atomically**. Close with a short note on how you verified each feature
end-to-end (the "feel" check included).

---

## Feature 1 — Living project blacklist

**Intent:** mutably exclude entire project trees from tracking without losing
history. Toggling a pattern off must re-surface sessions with no full re-wipe.

- New table `project_blacklist { pattern TEXT PRIMARY KEY, created_at TEXT }`.
  Not a hardcoded const. Seed once with **`udacity-project-reviews/**`**.
- Pattern = glob/prefix matching a parent dir **and all descendants**
  (`udacity-project-reviews/**` excludes the parent and every child session).
- Enforce in **both** places: `indexer.rs` skips matches at index time (they
  never enter the DB as tracked) **and** query paths in `lib.rs` apply a
  defensive filter, so re-indexing is idempotent and un-blacklisting re-surfaces
  without a wipe.
- Commands: `list_blacklist`, `add_blacklist_pattern`, `remove_blacklist_pattern`
  (+ ipc wrappers). Each with a live **match count** per pattern.
- UI: a small settings/manage panel — add/edit/remove live; changes take effect
  on the next query, no app restart. Match project-item styling in the sidebar;
  no chunky pills (see FRONTEND.md gap 4).
- **Definition of done:** blacklisting a live project makes its sessions vanish
  from the list within one query cycle; removing the pattern brings them back
  unchanged; the count is accurate; the panel reads calm, not like a firewall
  config.

## Feature 2 — Progressive-disclosure session cards

**Intent:** tiered reveal so the highest-signal info never requires a click. The
current card is already recap-led — evolve it, don't rip it out.

- **Tier 1 (always visible, glanceable):** recap goal one-liner (the headline,
  hero weight) · short project name · area-of-life chip · completion-% badge ·
  timestamp. Keep the `Sparkles` recap marker. Chips obey the cross-feature
  data rule: absent data renders nothing, cleanly.
- **Tier 2 (hover/expand):** recap summary, session duration, message count,
  tags. Expand animates with framer `layout` spring.
- **Tier 3 (click → `SessionDetail`):** full recap, transcript access, raw path,
  edit tags.
- Respect landmine 4: keep react-virtual windowing intact via `measureElement`;
  verify 60fps at full dataset scale.
- **Definition of done:** a cold glance answers "what, where, how done, when"
  with zero interaction; hover deepens without layout jank; expanded rows never
  overlap; the badge and area chip are subordinate garnish, not louder than the
  recap.

## Feature 3 — One-shot AI session tagging

**Intent:** "dig" a session and tag it near-instantly. See landmines 1–2 for
transport (local `claude` CLI headless, PATH-resolved, **not** a new HTTP
client).

- Prominent action in `SessionDetail` **and** a quick action on the card. One
  structured call over the session's recap + metadata, model
  **`claude-haiku-4-5`** (fast; it is in the `pricing` table).
- The call must return strict JSON, validated server-side; malformed → typed
  error, never a silent pass:
  ```json
  {
    "area_of_life": "one of the controlled list below",
    "project_short_name": "SHORT semantic name, never a long path",
    "goal_completed": true,
    "completion_pct": 0,
    "rationale": "one sentence"
  }
  ```
- Controlled `area_of_life` vocabulary (editable one-liner; default):
  **`Building` · `Research` · `Content` · `Ops` · `Personal`**.
- Persist on the session row (columns landed in the first migration).
  Re-running overwrites the **auto** fields only.
- **Mandatory UX states:** idle → loading (optimistic spinner, <2s target) →
  success (fields populate inline, editable) → error/timeout (retry button,
  never silent). Loading uses the accent, not a jarring color.
- **Manual-edit protection:** any hand-edited field is flagged and never
  clobbered by a future auto-tag unless the user explicitly re-runs. Surface the
  "manually edited" state quietly (a small dot/marker, not a banner).
- **Definition of done:** click → under ~2s the card gains an area chip + a real
  completion %; editing a field sticks across a re-tag; a failed call shows a
  retry, never an empty success; with the CLI missing, the error state names
  the fix.

## Feature 4 — Kanban board from completion %

**Intent:** a drag-and-drop board across **Planned / In Progress / Completed**,
reusing the same progressive-disclosure card (Tier 1).

- Lives behind a **view toggle in `TopBar`** (list | board) — plain component
  state, no routing dep.
- Default column from `completion_pct`: `0 → Planned`, `1–99 → In Progress`,
  `100 || goal_completed → Completed`. Untagged sessions (no %) stay off the
  board until tagged — the board is the triage surface for tagged work, not a
  second copy of the list.
- Dragging sets an explicit `kanban_status` that **overrides** the %-derived
  column (override-wins). Persist status **and** per-column order in the DB.
- A later auto-tag that changes % must **not** move a manually placed card
  (respect the override flag) — instead show a subtle "% changed" hint on the
  card.
- Drag via framer-motion or minimal HTML5 DnD — **no heavy new dep**; justify
  anything beyond that in a sentence before adding it.
- Empty columns render a clear, warm empty state (a line of guidance, not a bare
  icon — see FRONTEND.md gap 5), not a dead zone.
- **Definition of done:** drag persists across reload; a re-tag never yanks a
  placed card; columns feel like the same card family as the list (not a second
  design language); empty columns invite rather than confuse.

---

## Constraints (hard)

- Every backend capability = `#[tauri::command]` + `generate_handler![]` entry +
  typed `ipc.ts` wrapper + TS interface + DB migration (via landmine 3).
- Use only the existing design tokens and existing deps. No second accent, no
  new gray, no new framework.
- **No emojis** anywhere in code, UI copy, commits, or output.
- Incremental: 1 → 2 → 3 → 4, each independently working, atomically committed,
  verified per the protocol after each.

## Filled variables (for reuse in other repos, re-parameterize these)

| Variable | Value here |
|---|---|
| `BLACKLIST_SEED` | `udacity-project-reviews/**` |
| `TIER1_FIELDS` | recap goal one-liner · short project name · area-of-life chip · completion-% badge · timestamp |
| `TAGGING_MODEL` | `claude-haiku-4-5` |
| `AREAS_OF_LIFE` | `Building` · `Research` · `Content` · `Ops` · `Personal` |
