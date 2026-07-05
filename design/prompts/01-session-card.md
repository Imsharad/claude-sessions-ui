# Master prompt — Session card / row

JTBD: let the eye recognize "that session" among 700 in under a second.

```text
You are a senior product designer who specializes in dense, information-rich list UIs
(think Linear, Superhuman, GitHub PR lists). You design for keyboard-driven power users
who scan hundreds of rows, not casual browsers.

{{PASTE THE <context> BLOCK FROM _context.md HERE}}

<surface_context>
The card is the atomic unit of BOTH the session list and the kanban board (same
component, SessionCardView). Current implementation is recap-led with 3-tier
progressive disclosure: Tier 1 always visible (recap headline, short project, area
chip, completion badge, relative time), Tier 2 expands in place via hover chevron
(fuller recap + tag fields), Tier 3 is the detail pane. Rows are virtualized with
dynamic measurement, so variable heights are allowed but must settle instantly.
</surface_context>

<task>
Design 5 distinct variants of the session card/row, then generate each one as a
working, renderable mockup.

Each variant must occupy a DIFFERENT point on the density–richness spectrum. Cover at
minimum: (a) ultra-dense single-line row (~32px, table-like), (b) two-line compact
card, (c) comfortable card with recap-snippet preview, and use the remaining two slots
for genuinely different structural ideas (e.g. timeline-grouped, project-clustered,
status-led) — not color reskins of the same layout.
</task>

<requirements>
For every variant:
1. Information hierarchy — decide which 2-3 fields earn primary visual weight and why;
   demote or hide the rest behind hover/expand. State the reasoning in one sentence.
2. Scale honesty — show the card repeated at least 8 times with REALISTIC varied data,
   including these edge cases: a very long title that must truncate, an untagged
   session where ALL triage fields are null, a session with null gitBranch and no
   recap, an extreme value (900 messages / 14h duration / $40 cost), and a
   just-created session seconds old.
3. Search/filter integration — show how a free-text match is highlighted on the card,
   and ensure every filterable field (project, date, areaOfLife, kanbanStatus, pinned)
   is either visible on the card or clearly not needed for scan-recognition.
4. Interaction states — style hover, selected, and keyboard-focused states, plus the
   pinned state.
5. Board compatibility — the card must also work at ~280px column width; show one
   sample of each variant at that width.
6. Density math — state how many rows fit in an 800px-tall viewport.
7. Micro-craft — this component is seen thousands of times, so its details compound.
   Honor the DESIGN LANGUAGE explicitly: name the type sizes and weights used (recap
   headline vs metadata), use tabular numerals for time / cost / message counts, pick the
   ONE accent moment per card and say where it lives, decide how a truncated title ends
   (soft mask vs ellipsis), and specify the hover/expand transition as one line (property,
   duration, easing). Rows separate by hairline + spacing rhythm, never by boxing each
   card. The card at rest should read as calm; weight belongs only on the 2-3 primary
   fields.
</requirements>

<output_format>
1. One self-contained HTML file (inline CSS, no external assets) rendering all 5
   variants as labeled sections, each with its 8+ sample rows plus the narrow-width
   sample, so they can be compared side by side in a browser.
2. A comparison table scoring each variant 1-5 on: scan speed, information density,
   edge-case resilience, aesthetic restraint (calm at a glance, rewards a second look),
   implementation effort.
3. A final recommendation: which variant to ship as default, and whether a density
   toggle between two of them is worth it. Commit to an opinion.
</output_format>

Think step by step about the field hierarchy before writing any markup: for a user
scanning 700 sessions to find one from last Tuesday, which fields do their eyes
actually use? Design for that scan path first.

Taste pass — before finalizing, hold every variant against the DESIGN LANGUAGE and
remove one element from each that does not earn its place; name what you cut. The card
you ship should feel quiet in a stack of 700 and still reveal everything on a second
look.
```
