# Master prompt — Sidebar (project navigation + search)

JTBD: cut 700 sessions down to one project's worth in a single click, and keep the
user's 5 live projects zero-distance away.

```text
You are a senior product designer who specializes in navigation for dense productivity
tools (think Linear's project sidebar, Things' areas, Slack's channel rail). You design
for a keyboard-driven solo user with many parallel projects.

{{PASTE THE <context> BLOCK FROM _context.md HERE}}

<surface_context>
Current sidebar: "All sessions" entry, pinned projects (star toggle, persisted),
all projects grouped with session counts, a search input that drives the list's
free-text filter, and a blacklist manager (hide directory trees from indexing, with
two-source match-count preview and always-visible remove). Projects are derived from
projectDir; counts are per-project session counts. The user has ~5 active projects and
a long tail of dozens of stale ones.
</surface_context>

<task>
Design 4 distinct variants of the sidebar, then generate each as a renderable mockup.

Variants must differ in how they handle the active-few vs long-tail problem, covering
at least: (a) pinned + alphabetical tail (current baseline, refined), (b) recency-
ordered with automatic decay (stale projects collapse into a "dormant" group),
(c) area-of-life as the first hierarchy level with projects nested under
Building/Research/Content/Ops/Personal, (d) a minimal rail (icons/initials only) that
expands on hover or focus.
</task>

<requirements>
For every variant:
1. Long-tail honesty — mock with 5 hot projects and 25+ stale ones; show how the tail
   stays reachable without burying the hot five.
2. Counts and signals — decide what number (if any) sits next to each project: total
   sessions, untagged count, in-progress count. Justify in one sentence; a count the
   user never acts on is noise.
3. Search placement — search filters sessions, not projects; make that unambiguous in
   the design, and show the active-search state (input focused, query present, list
   filtered indicator).
4. Selected/hover/keyboard states for project entries, plus the pinned affordance
   (visible on hover vs always).
5. Blacklist entry point — keep it discoverable but out of the daily path; show where
   it lives.
6. Width discipline — each variant states its width in px and what happens to long
   project names.
7. Quiet-nav craft — the sidebar is glanced at constantly, so it must recede. Per the
   DESIGN LANGUAGE: the active-project indicator is restrained (a hairline, a dot, or a
   faint surface tint — not a heavy filled pill), counts are muted secondary text with
   tabular numerals, and hover/pin affordances fade in rather than snap. State the type
   scale for project names vs counts, and the exact treatment of the selected state. A
   sidebar that competes with the list for attention has failed.
</requirements>

<output_format>
1. One self-contained HTML file (inline CSS, no external assets), all 4 variants side
   by side at true width and ~600px height, with realistic project names of varying
   length.
2. A comparison table scoring each variant 1-5 on: hot-project access speed, long-tail
   findability, horizontal space cost, aesthetic restraint (recedes, quiet active state),
   implementation effort.
3. A final recommendation. Commit to an opinion, including whether area-of-life
   belongs in the sidebar at all or only in filters.
</output_format>

Think step by step first: the sidebar is glanced at hundreds of times a day but
actively used a handful of times. Optimize the glance (am I in the right scope?) over
the click.

Taste pass — before finalizing, squint at each variant: the hot five should surface by
weight and quiet, not by decoration. Remove one count, badge, or divider per variant
that the glance does not need, and name it. The winner should feel like furniture you
stop noticing.
```
