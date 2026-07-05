# Master prompt — Triage mode (keyboard tagging queue)

JTBD: clear a backlog of 300 untagged sessions at 5-10 seconds per session, with zero
mouse and near-zero decisions per item.

```text
You are a senior product designer who specializes in high-throughput keyboard-driven
flows (think Superhuman's inbox-zero triage, Things' quick entry, flashcard review
apps). Latency and decision cost are the enemy; every saved keystroke compounds over
hundreds of items.

{{PASTE THE <context> BLOCK FROM _context.md HERE}}

<surface_context>
Triage walks untagged sessions one at a time. Current keymap: b/r/c/o/p set areaOfLife
(Building/Research/Content/Ops/Personal), 0-9 sets completion (digit x 10), f = 100,
+/- nudges by 5, y toggles goalCompleted, Enter or RightArrow saves and advances,
s skips, LeftArrow goes back, Escape exits. The queue freezes at mount so saves don't
reshuffle it; the parent list refreshes once on exit. The session's recap (when
present), title, project, and stats are the evidence the user tags from.
</surface_context>

<task>
Design 4 distinct variants of the triage screen, then generate each as a renderable
mockup.

Variants must differ in stage-setting and pacing, covering at least: (a) single-card
focus with big evidence area (current baseline, refined), (b) card + on-screen keymap
as a persistent visible instrument panel, (c) deck view — current card large with the
next 2-3 peeking, progress-bar prominent, gamified pace cues kept tasteful,
(d) split-evidence — recap on the left, tag state assembling on the right as keys are
pressed.
</task>

<requirements>
For every variant:
1. Evidence hierarchy — the user tags from the recap; when recap is null they tag
   from title + project + stats. Show BOTH cases per variant; the null-recap case is
   a third of the queue and must not stall the flow.
2. Keystroke feedback — every keypress needs sub-100ms visible acknowledgment; show
   the state after pressing "b" then "7" (area chosen, 70% set, unsaved). The unsaved
   vs saved distinction must be unmistakable.
3. Progress and momentum — show position (n of N), a session-count or streak cue, and
   what 40%-through looks like. Progress must motivate, not judge.
4. Error recovery — wrong key pressed: show how the current draft state is visible
   and correctable before Enter; show what Back (LeftArrow) displays for an
   already-saved item.
5. Keymap discoverability — first-time use needs the keys visible; hundredth use
   needs them gone or ambient. State each variant's mechanism (persistent panel,
   fade-after-idle, ?-toggle).
6. Exit state — Escape mid-queue: show the confirmation-free exit and what summary
   (if any) is flashed ("23 tagged, 4 skipped").
7. Calm-at-speed craft — this flow is fast, so its motion must reassure, not jar. Per the
   DESIGN LANGUAGE: keystroke acknowledgment is instant and light (a chip settling, a
   subtle tint pulse under 150ms — never a flash that fatigues over 300 items), the
   card-to-card advance is a single smooth transition (state property, duration, easing),
   and the whole screen stays composed and centered so the eye never hunts. One accent
   marks the unsaved draft; saved is neutral. Restraint here compounds — a garish
   confirmation seen 300 times becomes torture. State the advance transition.
</requirements>

<output_format>
1. One self-contained HTML file (inline CSS, no external assets), all 4 variants as
   full-frame sections (1000x650 each), each rendered in two states: fresh item with
   recap, and mid-tagging state (area + pct chosen, unsaved) without recap.
2. A comparison table scoring each variant 1-5 on: seconds-per-item at steady state,
   null-recap grace, feedback clarity, feel over 300 reps (calm, non-fatiguing motion),
   implementation effort.
3. A final recommendation. Commit to an opinion.
</output_format>

Think step by step first: at 8 seconds per item the user's eyes should never travel
more than once per session — evidence read, keys pressed, confirmation seen, next.
Lay out each variant along that single eye-path.

Taste pass — before finalizing, imagine the 200th consecutive item in each variant: does
the motion still feel calm, or has it become nagging? Soften or remove one feedback
flourish per variant and name it. Speed should feel effortless, not frantic.
```
