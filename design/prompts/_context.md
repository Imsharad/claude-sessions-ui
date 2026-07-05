# Shared context block

Prepend this block (verbatim, inside the `<context>` tag) to every master prompt in this
directory. It is the single source of truth for app description, users, data model, and
constraints. Update it here, never inside individual prompts.

```text
<context>
APP: "Claude Sessions" — a Tauri + React desktop app for browsing, triaging, and
resuming a personal history of Claude Code sessions. Local-only: a Rust indexer scans
~/.claude session JSONL files into SQLite; the UI reads via IPC. Current scale: ~700
sessions, growing by 5-20/day. Single user.

PRIMARY USER: a solo power user with ADHD who runs many parallel projects. He uses the
tool to (a) re-find a specific past session ("that thing from last Tuesday"), (b) triage
untagged sessions into areas of life and completion states, (c) see what is in flight
across projects, and (d) resume a session in the terminal. He is keyboard-driven and
allergic to cognitive load: every screen must have one obvious focal point and must not
demand decisions it can make itself.

DATA MODEL (SessionCard, the unit of display):
- title
- displayProject (short project name), projectDir, cwd
- gitBranch (nullable)
- firstTs / lastTs (start and last-activity timestamps)
- messageCount, durationMs
- planMode (bool), pinned (bool)
- hasRecap (bool) + recap (nullable summary text — when present, the best scannable
  description of what the session actually did)
- inputToks / outputToks / cacheReadToks, costUsd + costSource
- Tag/triage fields, ALL nullable (null = untagged): areaOfLife (Building / Research /
  Content / Ops / Personal), projectShortName, goalCompleted (bool), completionPct
  (0-100), tagRationale, kanbanStatus (planned / in_progress / completed)

Roughly half of all sessions are untagged at any time; every design must render
gracefully with all triage fields null.

GLOBAL STATS available: totalSessions, totalMessages, token totals (input/output/cache),
totalCostUsd (split measured vs estimated), earliest/latest timestamps, nWithRecap,
nPlanMode. Per-session detail adds: recaps (sequence), todos with status, per-model
usage, filesTouched with counts, errors, per-turn token series.

TECH/DESIGN CONSTRAINTS: React + Tailwind, framer-motion for animation, lucide icons,
@tanstack/react-virtual for long lists. Dark-mode-first desktop app, macOS native feel
(top bar is the window drag region). No external component libraries. Existing design
tokens: bg-canvas / bg-surface, text-ink / ink-2 / ink-3, border-border, accent color.
Information-dense, Linear/Superhuman aesthetic — quiet chrome, typography does the work.

EXISTING SURFACES: TopBar (view switcher: Launcher / Analytics / Digest; launcher modes:
List / Board / Triage), Sidebar (project nav + search + pin + blacklist), SessionList
(virtualized recap-led cards, 3-tier progressive disclosure), SessionDetail (right pane,
420px), KanbanBoard (3 columns from completion/kanbanStatus, drag with override-wins),
TriageMode (keyboard-only tagging: b/r/c/o/p areas, 0-9 completion, Enter saves),
FirstRun (initial scan loading screen). Analytics and Digest are unbuilt stubs.

DESIGN LANGUAGE (the taste bar — every surface inherits this; the target is modern,
elegant, minimalist, and smooth, not merely dense-and-correct):
- Restraint first. Remove before you add. One accent color, spent as a single point of
  emphasis per view; everything else is neutral ink on quiet surface. If an element does
  not earn its pixels in the scan, hide it behind hover/expand or cut it.
- Typography does the hierarchy. One sans family. Build hierarchy from a small type scale
  (roughly 11 / 12 / 13 / 15 / 18px) and weight (regular / medium / semibold), never from
  size-spam — no more than three type sizes visible at once in a dense view. Tabular
  lining numerals for every stat, timestamp, and cost so columns align. Comfortable
  measure (~60-72ch) wherever recap prose is actually read.
- Spacing has rhythm. A 4px base grid; consistent vertical rhythm; align to a shared left
  edge. Negative space is a feature, not waste — separate with padding and hairlines, not
  boxes-inside-boxes. Density comes from removing chrome, not from cramming.
- Depth is subtle. Prefer 1px hairline borders and small surface-contrast steps over drop
  shadows and heavy fills. Elevation (overlays, popovers) uses one soft shadow plus a
  faint border, never a dramatic one. Small, consistent corner radius (6-8px); reserve
  pill shapes for genuine chips/toggles.
- Motion is purposeful and quick. 120-220ms; ease-out for enter/reveal, ease-in for exit;
  direct manipulation (drag, reorder, keystroke feedback) gets a light spring. Motion
  never blocks input, never bounces gratuitously, never loops for decoration. Height and
  opacity changes (expand, filter, regroup) animate without layout jank. Honor
  prefers-reduced-motion by falling back to instant, never to broken.
- Color carries meaning, not decoration. Area/status hues are low-chroma and consistent
  app-wide; success/error color appears only where it is literally true. Never two
  saturated colors competing for attention in one view.
- The feel to aim for: calm, confident, quiet — made by someone who sweated the details
  and then removed half of them. When torn between two options, ship the more restrained
  one and let type and spacing carry the weight.
</context>
```
