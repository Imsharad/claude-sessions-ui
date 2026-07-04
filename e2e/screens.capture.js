// QA screenshot capture — NOT part of the golden-master suite (filename does not
// match the *.e2e.js glob; run explicitly with --spec e2e/screens.capture.js).
// Drives the app through its views and saves PNGs for vision-model QA episodes.
// Defensive: a failed interaction still captures whatever state is on screen.
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const EPISODE_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'qa', 'screenshots', 'episode-1');
mkdirSync(EPISODE_DIR, { recursive: true });

const SETTLE_MS = 700; // let spring animations land before capturing

async function shot(name) {
  await browser.pause(SETTLE_MS);
  await browser.saveScreenshot(join(EPISODE_DIR, name));
}

/** Run a DOM interaction in-page; swallow failures so every shot still fires. */
async function act(fn) {
  try { await browser.execute(fn); } catch { /* capture current state anyway */ }
}

describe('episode-1 screenshot capture', () => {
  it('captures ten views', async () => {
    // Boot: wait for the shell.
    await browser.waitUntil(async () => (await browser.getTitle()) === 'Claude Sessions', {
      timeout: 30000,
      timeoutMsg: 'frontend did not load',
    });
    await browser.pause(1500); // initial data load

    // 01 — launcher, full session list
    await shot('01-launcher-list.png');

    // 02 — first card expanded in place (Tier 2)
    await act(() => document.querySelector('[title="Expand"]')?.click());
    await shot('02-card-expanded.png');
    await act(() => document.querySelector('[title="Collapse"]')?.click());

    // 03 — session selected, detail pane (Tier 3: recap hero + tags section)
    await act(() => document.querySelector('div[class*="cursor-pointer"][class*="rounded-lg"]')?.click());
    await shot('03-detail-pane.png');

    // 04 — search results
    const search = await $('[placeholder="Search sessions & recaps…"]');
    try { await search.setValue('refactor'); } catch { /* keep going */ }
    await shot('04-search-results.png');
    try { await search.clearValue(); } catch { /* keep going */ }

    // 05 — hidden projects (blacklist) panel open
    await act(() => document.querySelector('[title="Hidden projects"]')?.click());
    await shot('05-hidden-projects-panel.png');
    await act(() => document.querySelector('[title="Hidden projects"]')?.click());

    // 06 — kanban board view
    await act(() => document.querySelector('[title="Board view"]')?.click());
    await shot('06-board-view.png');

    // 07 — board with a card selected (detail pane in board mode), or bare board
    await act(() => document.querySelector('[draggable="true"]')?.click());
    await shot('07-board-card-selected.png');

    // 08 — analytics tab (stub)
    await act(() => {
      const btn = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Analytics');
      btn?.click();
    });
    await shot('08-analytics.png');

    // 09 — digest tab (stub)
    await act(() => {
      const btn = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Digest');
      btn?.click();
    });
    await shot('09-digest.png');

    // 10 — back to launcher, list view, filtered to the first sidebar project
    await act(() => {
      const btn = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Launcher');
      btn?.click();
    });
    await act(() => document.querySelector('[title="List view"]')?.click());
    await act(() => {
      // second nav item = first real project (first is "All sessions")
      const items = document.querySelectorAll('nav > div[class*="cursor-pointer"]');
      (items[1] || items[0])?.click();
    });
    await shot('10-project-filtered.png');
  });
});
