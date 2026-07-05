# Master prompt — Top bar (view switcher + global stats + reindex)

JTBD: switch working modes instantly and answer "is my index fresh, what has all this
cost me" at a glance — while staying quiet enough to ignore the other 99% of the time.

```text
You are a senior product designer who specializes in application chrome for macOS
desktop tools (think Linear's header, Raycast, Arc's toolbar). Chrome must be
functional, native-feeling, and nearly invisible.

{{PASTE THE <context> BLOCK FROM _context.md HERE}}

<surface_context>
Current top bar: app title, view switcher (Launcher / Analytics / Digest), launcher
mode switcher (List / Board / Triage) shown only in Launcher view, global stats
summary (sessions, tokens, cost), reindex button with spinner state. The entire bar is
the Tauri window drag region on macOS — every interactive element must coexist with
window-dragging. IndexStatus provides lastScanTs/lastScanMode/sessionCount/recapCount;
GlobalStats provides totals including measured-vs-estimated cost split.
</surface_context>

<task>
Design 4 distinct variants of the top bar, then generate each as a renderable mockup.

Variants must differ in how they allocate the bar's three jobs (navigation, status,
action), covering at least: (a) everything-inline (current baseline, refined),
(b) stats demoted to a hover/click popover leaving only nav + reindex, (c) segmented
two-row or split layout separating mode switching from status, (d) command-bar-first —
the bar is minimal and a Cmd+K style switcher carries navigation.
</task>

<requirements>
For every variant:
1. Hierarchy of the two switchers — view (Launcher/Analytics/Digest) vs launcher mode
   (List/Board/Triage) are different levels; the design must make the nesting legible,
   not present six equal tabs.
2. Stats honesty — decide which of the available stats earn permanent visibility (if
   any) vs on-demand. Show the measured-vs-estimated cost distinction somewhere
   truthful. Justify each always-visible number in one sentence.
3. Reindex states — idle, reindexing (spinner + what the rest of the bar does), just
   finished (how freshness is communicated: "indexed 2m ago" or equivalent).
4. Drag-region discipline — mark which regions are draggable vs interactive; no dead
   zones wider than 100px between interactive clusters.
5. macOS traffic-light clearance on the left; state the bar height in px (36-48 range)
   and keep all type at 11-13px.
6. Keyboard — show the shortcuts for view/mode switching adjacent to or discoverable
   from the controls.
7. Invisible-chrome craft — this bar succeeds by disappearing. Per the DESIGN LANGUAGE:
   all type 11-13px, one weight step between active and inactive, the active view marked
   by the faintest means that still reads (underline, tint, or hairline — not a filled
   tab row). Stats sit in muted tabular numerals; the accent color appears at most once.
   The reindex spinner and the "indexed 2m ago" freshness cue animate in softly and never
   jitter the bar's layout. State the resting vs active treatment of the switchers.
</requirements>

<output_format>
1. One self-contained HTML file (inline CSS, no external assets), all 4 variants
   stacked full-width (1200px frame), each shown in two states: idle and reindexing.
2. A comparison table scoring each variant 1-5 on: mode-switch speed, status
   legibility, quietness (ignorability), drag-region safety, aesthetic restraint
   (refined, weightless chrome), implementation effort.
3. A final recommendation. Commit to an opinion.
</output_format>

Think step by step first: chrome earns its pixels by being ignorable. Start from what
can be REMOVED from the current bar, then design the variants.

Taste pass — before finalizing, ask of each variant: could a first-time user forget the
bar is there within a minute? Cut the loudest remaining element per variant and name it.
The bar should feel like part of the window, not a toolbar bolted on.
```
