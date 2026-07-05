# Master prompt — Session detail pane

JTBD: answer "what happened in this session and do I want to resume it" in one read,
with the recap doing the heavy lifting.

```text
You are a senior product designer who specializes in reading panes and inspector
panels (think Linear's issue panel, Superhuman's reading pane, GitHub's PR sidebar).
You balance a comfortable reading column against dense metadata.

{{PASTE THE <context> BLOCK FROM _context.md HERE}}

<surface_context>
The detail pane renders in two containers: a fixed 420px right pane beside the list,
and a right-anchored 420px overlay above the kanban board (Escape dismisses). Data per
session: the full SessionCard, recap sequence (possibly multiple, one final), todos
with status (final state of the session's task list), per-model usage rows (tokens +
cost per model), filesTouched with edit counts, errors, per-turn token series, and the
Resume action (opens the session in the terminal). Loading shows a light skeleton.
Recap is currently the hero, set at reading width. Tag editing (area, completion,
goal) is available here too.
</surface_context>

<task>
Design 4 distinct variants of the detail pane at exactly 420px width, then generate
each as a renderable mockup.

Variants must differ in information architecture, covering at least: (a) recap-first
scroll (current baseline, refined — everything below the recap in fixed order),
(b) sectioned with sticky mini-tabs or anchors (Recap / Work / Usage / Files),
(c) action-first — Resume and tag editing pinned top, content below, (d) split
hero+ledger — recap as prose up top, everything else as a compact key-value ledger.
</task>

<requirements>
For every variant:
1. The 5-second read — what does the user know after 5 seconds without scrolling?
   State it; the answer must include what the session did and its completion state.
2. Content honesty — mock THREE sessions: (i) rich (long recap, 12 todos mixed
   status, 3 models, 20 files, 2 errors), (ii) sparse (no recap, 4 messages, nothing
   tagged — the pane must not look broken), (iii) mid with one recap and a few todos.
   Render all three per variant or show the two extremes if space demands.
3. Resume prominence — Resume is the only action with an external effect; it must be
   reachable without scrolling in every variant and every content case.
4. Todos as evidence — todos show what was finished vs abandoned; design their done /
   in-progress / pending states to be countable at a glance.
5. Usage truthfulness — per-model cost rows must show costSource (measured vs
   estimated) without dominating; files and errors are reference material, not
   headline.
6. Overlay parity — note anything that changes in board-overlay mode (close
   affordance, shadow/elevation); design must survive both containers unchanged.
7. Reading craft — this is the one pane read as prose, so typography carries it. Per the
   DESIGN LANGUAGE: set the recap at a comfortable measure and generous leading (state
   the size / line-height / color), give sections a clear but quiet rhythm rather than
   boxed panels, render metadata as an aligned ledger with tabular numerals, and make the
   loading skeleton a calm echo of the layout (not gray blocks). Resume is the single
   accent element; everything else stays neutral. The overlay elevates with one soft
   shadow + faint border, per the DESIGN LANGUAGE, never a dramatic drop.
</requirements>

<output_format>
1. One self-contained HTML file (inline CSS, no external assets), all 4 variants as
   420px-wide columns side by side, each populated with the rich case and at least
   one alternate case, scrollable to true length.
2. A comparison table scoring each variant 1-5 on: 5-second comprehension, resume
   reachability, sparse-session grace, reading comfort / typographic craft, implementation
   effort.
3. A final recommendation. Commit to an opinion.
</output_format>

Think step by step first: the pane is read after the card already won the scan — so
never repeat what the card said; add what it couldn't say. List what the card already
communicates, subtract it, and design around the remainder.

Taste pass — before finalizing, read the recap in each variant as if it were an email:
does the type invite reading or resist it? Remove one panel border, label, or divider
per variant and let whitespace do that job instead; name what you cut.
```
