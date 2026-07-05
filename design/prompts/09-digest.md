# Master prompt — Digest view (greenfield)

JTBD: reconstruct "what did I actually do this week" in two minutes — a readable
narrative of past days, not another list to manage.

```text
You are a senior product designer who specializes in journal and review surfaces
(think Granola's daily notes, Reflect, a well-designed changelog). You design for
reading and remembering, not for triage — this surface has no required actions.

{{PASTE THE <context> BLOCK FROM _context.md HERE}}

<surface_context>
Digest is an UNBUILT view — a stub behind the top bar's view switcher. Design it from
scratch. The backend provides DigestDay groups: for each day, entries with sessionId,
title, project, lastTs, recap (nullable), messageCount. Assume the frontend can also
join full SessionCard data (area, completion, cost) by id. Cadence expectation: the
user opens this a few times a week, and for a weekly review.
</surface_context>

<task>
Design 3 distinct variants of the digest, then generate each as a renderable mockup
covering 5 days of realistic data (2 heavy days of 8+ sessions, 2 light days of 1-3,
one empty day).

Variants must differ in narrative form: (a) chronological journal — day headers,
sessions as prose-led entries, recaps set for reading, (b) day-summary-first — each
day compresses to a synthesized headline (projects touched, sessions, cost, wins)
that expands to entries, (c) week-at-a-glance — a 7-column week grid with density
signals, one selected day's detail below.
</task>

<requirements>
For every variant:
1. Reading gravity — recaps are the content; typography (measure, size, leading)
   must make 20 recaps in a sitting comfortable. State your type choices.
2. Day texture — heavy, light, and empty days must each look intentional; an empty
   day is a fact ("no sessions"), not an error state.
3. Cross-project weave — a day usually spans 3+ projects; show how project identity
   stays legible inside a day without fragmenting the reading flow into sub-lists.
4. Null-recap entries — render title + stats gracefully; decide whether recap-less
   sessions are demoted (smaller, grouped trailing line) and defend it.
5. Anchors — jumping to "last Tuesday" must take one interaction; show the
   navigation mechanism (date strip, week pager, sticky day rail).
6. Exit to detail — each entry links to the session detail; show the affordance
   without designing the detail pane.
7. Editorial craft — this is the app's one reading surface, so it should feel like a
   well-set page, not a data view. Per the DESIGN LANGUAGE, but leaning warmer: give
   recaps a genuine reading measure and leading, use restrained scale contrast between day
   headers and body, and let generous whitespace mark the rhythm of days-as-chapters.
   Day headers are typographic, not colored bars; project identity reads as a quiet inline
   mark, not a badge cluster. The empty day is composed and intentional. Motion is minimal
   — a gentle reveal on expand, nothing that performs. State the type ramp (day header /
   entry title / recap body) and the measure in ch.
</requirements>

<output_format>
1. One self-contained HTML file (inline CSS, no external assets), all 3 variants as
   full-page labeled sections (1100px frame), each with the full 5-day sample.
2. A comparison table scoring each variant 1-5 on: reading comfort / typographic craft,
   week reconstruction speed, heavy-day handling, emotional quality (does reviewing feel
   good), implementation effort.
3. A final recommendation. Commit to an opinion, including whether digest should
   open to today or to the last day with activity.
</output_format>

Think step by step first: this is the one surface where the user is REMEMBERING, not
searching or triaging. Optimize for narrative reconstruction — days as chapters,
recaps as sentences — and resist importing list-density instincts from the launcher.

Taste pass — before finalizing, read a full heavy day in each variant: does it feel like
a page worth reading or a report worth skimming? Remove one badge, count, or rule per
variant and let whitespace and type carry the structure instead; name what you cut.
```
