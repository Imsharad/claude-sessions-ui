# Master prompt — Analytics view (greenfield)

JTBD: answer "where is my Claude time and money actually going, and is it paying off"
— monthly-glance cadence, not a live ops dashboard.

```text
You are a senior product designer who specializes in personal-analytics surfaces
(think GitHub's contribution insights, RescueTime done tastefully, Linear Insights).
You are hostile to vanity dashboards: every chart must change a decision or it goes.

{{PASTE THE <context> BLOCK FROM _context.md HERE}}

<surface_context>
Analytics is an UNBUILT view — a stub behind the top bar's view switcher. Design it
from scratch. Available data across ~700 sessions: per-session tokens (input/output/
cache-read), costUsd with measured-vs-estimated source, durations, message counts,
timestamps (per-session first/last plus per-turn series), project, gitBranch,
planMode, and for the tagged half: areaOfLife, completionPct, goalCompleted,
kanbanStatus. GlobalStats gives lifetime totals. No external data, no goals/budgets
feature yet.
</surface_context>

<task>
Design 3 distinct variants of the analytics view, then generate each as a renderable
mockup with plausible fake data at realistic magnitudes (hundreds of sessions, tens
of millions of tokens, hundreds of dollars).

Variants must answer different primary questions: (a) spend-led — cost over time,
cost by project, cost by model, measured-vs-estimated honesty front and center,
(b) output-led — sessions and completion by area of life, goal completion rate,
where effort goes vs where it finishes, (c) rhythm-led — when the user works
(hour-of-day / day-of-week heatmap), session length distribution, streaks and gaps.
</task>

<requirements>
For every variant:
1. One headline — each variant opens with a single sentence-sized stat that answers
   its primary question ("$142 this month, 60% on brain"). Everything else supports
   it.
2. Chart discipline — maximum 4 visualizations per variant; every one gets a
   one-sentence caption stating the decision it informs. No chart without one.
3. Honest uncertainty — estimated costs are visually distinct from measured
   everywhere they appear; untagged sessions appear as an explicit "untagged" slice,
   never silently dropped.
4. Time control — one range selector (7d / 30d / 90d / all); show the 30d state.
   State what each chart does when the range contains sparse data.
5. Drill path — clicking any project/area/slice should conceptually lead back to the
   filtered session list; indicate the affordance without building it.
6. Charts as inline SVG or CSS — keep them simple (bars, lines, heatmap cells);
   dark-mode-first palette consistent with the app tokens.
7. Chart craft — minimalism is the whole point; a vanity dashboard is a failure of taste,
   not just of utility. Per the DESIGN LANGUAGE: strip chartjunk (no gridlines that don't
   aid reading, no legends a direct label could replace, no 3D, no gradients-for-show),
   let one accent carry the primary series while the rest stay neutral ink, and set all
   figures in tabular numerals. Axis labels and captions are quiet secondary text.
   Estimated data is shown with a consistent restrained treatment (hatch, outline, or
   muted fill) everywhere. The headline stat is the one large moment on the page; charts
   support it in a calm grid. State the palette and how estimated-vs-measured reads.
</requirements>

<output_format>
1. One self-contained HTML file (inline CSS/SVG, no external assets), all 3 variants
   as full-page labeled sections (1200px frame).
2. A comparison table scoring each variant 1-5 on: decision value, glanceability,
   data honesty, aesthetic restraint (minimal, chartjunk-free, one accent), implementation
   effort.
3. A final recommendation: which variant ships as v1, and which single element from
   each losing variant is worth stealing into it. Commit to an opinion.
</output_format>

Think step by step first: list the 5 questions this user could plausibly act on
(cancel nothing — he can only change habits and project allocation), then design only
for those. A number he cannot act on is decoration.

Taste pass — before finalizing, delete the least-actionable chart in each variant and
confirm the page is better for it; name what went. The v1 that ships should look like a
quiet report a careful person made, not a dashboard trying to impress.
```
