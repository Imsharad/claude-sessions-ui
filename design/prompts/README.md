# design/prompts — master prompts for every surface

One detailed, locked master prompt per screen/view/component of Claude Sessions.
Each prompt is self-contained once you prepend the shared context block from
`_context.md`. Paste the combined text into any strong model (or run it in a Claude
Code session) and you get comparable, renderable design variants for that surface.

## How Stanford would start (and how this directory maps to it)

The Stanford d.school design-thinking process is five modes — Empathize, Define,
Ideate, Prototype, Test — and the honest answer is: it does NOT start with prompts.
It starts with the user. Prompts enter at stage 3. The discipline is refusing to
generate pixels before stages 1-2 are written down.

1. **Empathize** — watch the real usage, not the imagined one. For a redesign of this
   tool: which sessions does the user actually re-open? How does he search — by
   project, by time, by half-remembered phrase? Where does he stall (untagged
   backlog)? Output: observations, not requirements.
   *Here:* the PRIMARY USER paragraph in `_context.md` is the compressed empathy
   artifact. When it drifts from reality, fix it there first — every downstream
   prompt inherits the fix.

2. **Define** — compress observations into a point-of-view / job-to-be-done statement
   per surface ("scanning 700 sessions to find one from last Tuesday" is the list's
   JTBD; "clear the untagged queue at zero cognitive cost" is triage's). Output: one
   sharp sentence per surface.
   *Here:* each prompt's `<context>` addendum + the closing "scan path" instruction
   encode that surface's JTBD. This is the stage the prompts actually capture — a
   master prompt IS a frozen Define artifact.

3. **Ideate** — diverge on purpose. Many structurally different options, not one
   option polished. The d.school rule: defer judgment, go for volume, make variants
   genuinely different.
   *Here:* every prompt demands N variants occupying different points on an explicit
   spectrum (density, hierarchy, structure) and bans "color reskins of the same
   layout." This is what the model is good at — cheap, fast divergence.

4. **Prototype** — build the cheapest thing that can be experienced. Not production
   code: throwaway artifacts you can look at and react to.
   *Here:* every prompt's output format is a single self-contained HTML file with
   realistic data at realistic scale. Rendering in a browser IS the prototype.

5. **Test** — put the prototype in front of the user, at honest scale, with honest
   data, and watch. Feed what breaks back into Define.
   *Here:* the "scale honesty" and edge-case requirements (8+ rows, null tag fields,
   overflow titles, extreme values) are pre-baked tests; the comparison table +
   forced recommendation in each prompt is the judgment step. Your reaction to the
   rendered board closes the loop — update `_context.md` or the prompt and re-run.

The loop is not linear. A redesign typically cycles Define → Ideate → Prototype →
Test per surface, several times, and each cycle is one prompt execution. That is why
these are versionable files in the repo and not chat history.

## Usage

1. Copy the `<context>` block out of `_context.md`.
2. Open the surface's prompt file; paste the context block where marked.
3. Run it in a fresh model session (fresh = no anchoring on this repo's current CSS).
4. Render the HTML output, react, edit the prompt or context, re-run.
5. When a variant wins, hand the winning HTML + the prompt to an implementation
   session against the real components.

## Index

| File | Surface | Status in app |
|---|---|---|
| `01-session-card.md` | Session card/row (unit of List + Board) | built, iterating |
| `02-session-list.md` | Virtualized list pane | built, iterating |
| `03-sidebar.md` | Project nav + search + blacklist | built, iterating |
| `04-topbar.md` | View switcher + global stats + reindex | built, iterating |
| `05-session-detail.md` | Right detail pane / board overlay | built, iterating |
| `06-kanban-board.md` | 3-column board for tagged work | built, iterating |
| `07-triage-mode.md` | Keyboard-only tagging queue | built, iterating |
| `08-analytics.md` | Analytics view | unbuilt stub — greenfield |
| `09-digest.md` | Digest view (daily rollup) | unbuilt stub — greenfield |
| `10-first-run.md` | First-scan loading screen | built, low priority |

Conventions all prompts share: senior-designer role framing, XML-tagged sections,
variants on an explicit spectrum, realistic data + null-field edge cases, interaction
states, single self-contained HTML deliverable, scored comparison table (including an
aesthetic-restraint dimension), and a forced opinionated recommendation. Every prompt
also inherits the shared DESIGN LANGUAGE block in `_context.md` — the taste bar for
modern, elegant, minimalist, smooth output — and closes with a "taste pass" that forces
subtraction before finalizing. No emojis anywhere in outputs.
