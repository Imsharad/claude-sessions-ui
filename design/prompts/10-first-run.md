# Master prompt — First-run / scanning screen

JTBD: make the 5-7 second cold-start scan feel intentional and trust-building — the
app's first impression and its only one.

```text
You are a senior product designer who specializes in first-run and loading
experiences (think Arc's onboarding, Linear's first sync, well-crafted installers).
You know a loading screen is a promise about the product's quality.

{{PASTE THE <context> BLOCK FROM _context.md HERE}}

<surface_context>
On first launch the Rust indexer scans all ~/.claude session files (~5-7s for 700
sessions); subsequent launches run an incremental scan behind a brief "Checking for
new sessions..." state. Current screen: centered app icon, title, status message,
spinner. Available live signals the backend could expose during scan: files
discovered, sessions parsed, projects found, running token/cost totals. There is no
other onboarding — after the scan the user lands directly in the full launcher.
</surface_context>

<task>
Design 3 distinct variants of the first-run screen, then generate each as a
renderable mockup showing early (10%), mid (60%), and done (transition) moments.

Variants must differ in what they do with the wait: (a) calm-minimal — refined
current baseline, no numbers, just confident motion, (b) live-inventory — the scan
narrates its discoveries as they happen ("412 sessions - 9 projects - $61 tracked"),
building anticipation for the data, (c) primer — use the seconds to teach the three
core surfaces (list, board, triage) with one line + tiny visual each while a quiet
progress bar runs.
</task>

<requirements>
For every variant:
1. Perceived speed — the design must make 6 seconds feel shorter, never longer; no
   fake progress bars that stall at 90%. State the pacing trick used.
2. Incremental-scan reuse — show the compressed variant for the ~1s warm-start check
   ("Checking for new sessions...") — same design language, no ceremony.
3. Failure honesty — show the state when the scan errors mid-way (permissions,
   corrupt file): message, retry affordance, and the path to continue with partial
   data.
4. Motion restraint — describe animations in a sentence each (what moves, duration,
   easing); nothing loops aggressively; respects prefers-reduced-motion.
5. The landing cut — describe the transition from done-state into the launcher; the
   first frame of the launcher should feel caused by the scan, not a hard swap.
6. First-impression craft — this screen IS the promise of the product's quality, so
   motion restraint is the whole game. Per the DESIGN LANGUAGE: one confident piece of
   motion, ease-out, no aggressive loops, no fake progress that stalls at 90%. Center a
   single focal point with generous whitespace; if numbers narrate, set them in tabular
   figures that tick smoothly rather than flickering. The accent appears once. The landing
   cut into the launcher is a soft cross-dissolve or content settle, not a jump. State
   every animation in one line (what moves, duration, easing) and confirm the
   reduced-motion fallback is graceful, not empty.
</requirements>

<output_format>
1. One self-contained HTML file (inline CSS, no external assets), all 3 variants as
   labeled sections, each with its three moments (10% / 60% / done) side by side in
   800x500 frames, plus the warm-start compact state.
2. A comparison table scoring each variant 1-5 on: perceived speed, trust built,
   reusability for warm starts, motion restraint / craft, implementation effort.
3. A final recommendation. Commit to an opinion.
</output_format>

Think step by step first: the user sees this screen exactly once at full length —
weigh what is worth saying in six unskippable seconds, and remember that the
incremental one-second version will be seen hundreds of times.

Taste pass — before finalizing, ask whether each variant would still feel calm and
premium on the 200th warm-start glimpse. Remove one moving or shining element per variant
and name it. Confidence here is stillness, not spectacle.
```
