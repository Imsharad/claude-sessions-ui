# Claude Sessions UI — Master Frontend Context

*The single source of truth for every frontend decision. If a change conflicts
with this doc, change the doc first. Drives the meta-prompt for all UI work.*

---

## What this product is
A native Mac app that turns 690 hidden Claude Code session files into a
**visible, browsable, resumable** history. Three jobs: (1) find & resume any
session, (2) understand your usage (cost/tokens/time), (3) organize sessions by
what actually happened (recaps). The competitor is "nothing" — today these
sessions vanish into `~/.claude/projects/`.

## Who it's for
One power user (Sharad) who lives in Claude Code all day. ADHD-leaning, so the
UI must reduce cognitive load: scannable, decisive, never noisy. The delight
bar is "Mail.app/Linear gene," not "generic admin dashboard."

## The north-star feeling
> **"Creamy, calm, confident."** Like opening a beautifully designed notebook of
> your own work — warm paper, soft depth, type that breathes. Not a gray SaaS
> dashboard. Not stark. Not busy.

## The five design pillars (non-negotiable)
1. **Warm, not gray.** Canvas is paper (#faf9f7-ish), surfaces are cream-white.
   Ink is warm dark, never pure black or blue-gray. If it reads "gray," it failed.
2. **Recap is the hero.** The auto-generated recap is the headline of every
   card and the centerpiece of detail. Title is secondary. A sparkle (✨) marks
   recap-bearing sessions.
3. **One accent, used sparingly.** Soft blue for selection/active/links only.
   Never for decoration. Semantic colors (green/amber/red) only for status.
4. **Soft depth, no hard edges.** 12–16px radii, layered shadows, gentle hover
   lift. Motion is spring-physics (Framer), never linear.
5. **Honest density.** Show real metadata (project, branch, time, cost, msgs)
   but group it so the eye scans it in one pass, not as a cramped run-on.

## Current state (verified working)
- Backend: 42/42 integration checks pass; wire format proven (serde camelCase).
- 690 sessions, 543 recaps, $16,848 est cost, 90.45M tokens indexed.
- Launcher renders: top bar, sidebar (projects + pinning + search), virtualized
  recap-led card list, detail pane with recap/todos/usage/files.
- Stubbed: Analytics tab, Digest tab (P4/P5), Resume live-test (P3).

## Gaps the screenshot exposes (refinement backlog, priority order)
1. **Warmth didn't land.** Screenshot reads gray (#f8f8f8), not warm cream.
   Push canvas/surfaces warmer; verify on screen.
2. **Title leakage.** Card 3 shows raw `<instructions><references>` XML as a
   title — prompt content, not a real title. Sanitize: strip XML/angle-bracket
   content, fall back to recap or first user message.
3. **Card hierarchy is muddy.** Recap headline and title subtitle are too close
   in weight → eye doesn't know what's primary. Sharpen: recap darker + slightly
   larger, title lighter + smaller.
4. **"All sessions" pill is heavy.** Chunky blue tag breaks the sidebar rhythm.
   Match project-item styling.
5. **Empty right pane is a dead zone.** When nothing's selected, show something
   useful (tip, keyboard hints, or a hero stat) — not a lone icon + sentence.
6. **Metadata row runs together.** Cramped, no grouping. Use dot separators and
   group (project · branch) | (time · msgs · duration) | (cost · tokens).
7. **Accent over-saturated on screen.** Selection blue looks ~#4285f4 (harsh),
   not the intended #4c7bf5 (soft). Verify and soften.

## Out of scope for the refinement pass
- Analytics/Digest views (separate phases).
- Resume live-test (P3 — needs real Terminal interaction).
- New features. This pass is *visual + UX polish on the Launcher only*.

## The meta-prompt (use this before any frontend change)
> "Given the master context in FRONTEND.md: this change serves pillar [N] and
> fixes gap [M]. It makes the app feel [warmer/calmer/more scannable]. It does
> not add visual noise, harden edges, or introduce a second accent. After the
> change, the screenshot should read less [gray/busky/cramped] and more
> [cream/calm/breathable]. If it doesn't, revert."
