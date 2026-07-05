# Master prompt — Kanban board

JTBD: show what is actually in flight across all projects, and let the user correct
the machine's completion guess with one drag.

```text
You are a senior product designer who specializes in board and pipeline UIs (think
Linear's board view, Trello done right, Height). You design boards that stay honest
at 100+ cards, not demo boards with six.

{{PASTE THE <context> BLOCK FROM _context.md HERE}}

<surface_context>
The board shows TAGGED sessions only, in three columns: Planned (completionPct 0),
In Progress (1-99), Completed (100 or goalCompleted). An explicit drag sets
kanbanStatus which overrides the %-derived column (override-wins). Cards are the same
SessionCardView as the list — same design family. Drag is native HTML5 DnD with
fractional ordering. Selecting a card opens the 420px detail overlay; the board
compresses its columns to keep all three co-visible rather than clipping. Untagged
sessions are absent by design — triage mode is the intake.
</surface_context>

<task>
Design 4 distinct variants of the board layout and its column system, then generate
each as a renderable mockup using a compact placeholder card.

Variants must differ structurally, covering at least: (a) classic three fixed columns
(current baseline, refined), (b) project-swimlaned — columns crossed with horizontal
project lanes, collapsible per lane, (c) weighted columns — In Progress gets the
widest column and richest cards, Planned/Completed compress to dense rows,
(d) a wildcard of your own reasoning (e.g. Completed as a collapsed drawer/archive
strip, age-decayed opacity, WIP-limit signals).
</task>

<requirements>
For every variant:
1. Scale honesty — mock with 8 Planned, 14 In Progress, 60 Completed across 6
   projects. Completed dominating is the real distribution; the variant must handle
   it without three-screen scrolls in one column.
2. Column identity — each column header shows count and, where meaningful, a second
   signal (e.g. oldest in-progress age). Justify any number you show.
3. Override legibility — a card whose column came from an explicit drag (kanbanStatus)
   vs derived from completionPct: decide whether that difference is visible, and
   defend the decision in one sentence.
4. Drag states — render a card mid-drag, the drop indicator between cards, and the
   column-highlight state. Specify what the drop does to completionPct (nothing —
   override only) so the mockup doesn't lie.
5. Detail-overlay compression — show each variant at full width AND compressed with a
   420px overlay present; all columns must remain co-visible in both.
6. Cross-project scanning — the user asks "what's in flight in project X?"; show the
   mechanism (lane, filter chip, hover-dim) that answers it without leaving the board.
7. Board craft — drag is the emotional core, so it must feel physical. Per the DESIGN
   LANGUAGE: the lifted card gets a light spring and one soft shadow (not a heavy one),
   the drop indicator is a single crisp hairline that eases into place, and the column
   receiving the card tints faintly rather than glowing. Column headers are quiet
   (name + muted tabular count, hairline underline — not colored banners). The Completed
   pile reads as calm archive (lower contrast, tighter rows), never as visual noise
   competing with In Progress. State the drag transition (property, duration, easing).
</requirements>

<output_format>
1. One self-contained HTML file (inline CSS, no external assets), all 4 variants as
   full-width labeled sections (1200px frame), each with the full-width and
   overlay-compressed states.
2. A comparison table scoring each variant 1-5 on: in-flight visibility, completed-
   pile handling, drag feel (affordance clarity + motion craft), density at 82 cards,
   implementation effort on top of the existing DnD.
3. A final recommendation. Commit to an opinion.
</output_format>

Think step by step first: this board's center of gravity is IN PROGRESS — Planned is
a queue and Completed is a trophy shelf. Allocate pixels proportional to decision
value, not column symmetry.

Taste pass — before finalizing, watch a mental drag from Planned to Completed in each
variant: does it feel smooth and physical, or clunky and loud? Mute one element per
variant (a header color, a badge, a border) so In Progress clearly owns the eye; name
what you toned down.
```
