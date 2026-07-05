// Digest-week backfill + re-verification — NOT part of the golden-master suite
// (filename does not match the *.e2e.js glob; run explicitly with --spec).
// Clicks the real "Digest week" action (LLM backfill over the 7d window), waits
// for completion, then captures the populated L0: thread TOC rows with
// dot-strips, day composites with open-loop counts, worked-on lines at L1, and
// the thread click-to-filter behavior. Evidence in qa/screenshots/timeline-digest/.
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const OUT_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'qa', 'screenshots', 'timeline-digest');
mkdirSync(OUT_DIR, { recursive: true });

const transcript = [];

async function snap(step) {
  const state = await browser.execute(() => {
    const body = [...document.querySelectorAll('.overflow-y-auto')].find((el) =>
      el.querySelector('[data-trow]'),
    );
    return {
      threadRows: document.querySelectorAll('[data-trow^="thread:"]').length,
      dayRows: document.querySelectorAll('[data-trow^="day:"]').length,
      expandedRegions: document.querySelectorAll('[data-owner]').length,
      l0Fits: body ? body.scrollHeight <= body.clientHeight + 1 : null,
      reportLine:
        [...document.querySelectorAll('span')]
          .map((s) => s.textContent)
          .find((t) => /new · \d+ cached/.test(t ?? '')) ?? null,
      politeAnnouncement:
        document.querySelector('[aria-live="polite"]')?.textContent || null,
      firstThreadTitle:
        document.querySelector('[data-trow^="thread:"]')?.getAttribute('title') ?? null,
    };
  });
  transcript.push({ step, ...state });
}

describe('timeline digest-week backfill', () => {
  it('backfills the window, then verifies threads + populated L0/L1', async function () {
    this.timeout(900000); // the backfill is a real LLM batch — allow up to 15m

    await browser.waitUntil(async () => (await browser.getTitle()) === 'Claude Sessions', {
      timeout: 30000,
      timeoutMsg: 'frontend did not load',
    });
    await browser.pause(1500);

    await browser.execute(() => {
      [...document.querySelectorAll('button')]
        .find((b) => b.textContent.trim() === 'Timeline')
        ?.click();
    });
    await browser.waitUntil(
      async () => await browser.execute(() => !!document.querySelector('[data-trow]')),
      { timeout: 20000, timeoutMsg: 'timeline rows never rendered' },
    );
    await browser.pause(500);
    await snap('before backfill');

    // Kick off the week backfill and wait for the button to settle back.
    await browser.execute(() => {
      [...document.querySelectorAll('button')]
        .find((b) => b.textContent.includes('Digest week'))
        ?.click();
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          () =>
            ![...document.querySelectorAll('button')].some((b) =>
              b.textContent.includes('Digesting week'),
            ),
        ),
      { timeout: 840000, interval: 5000, timeoutMsg: 'backfill never finished' },
    );
    await browser.pause(1500);
    await snap('after backfill');
    await browser.saveScreenshot(join(OUT_DIR, '01-L0-populated.png'));

    // L1 with worked-on lines: expand the newest day.
    await browser.execute(() => document.querySelector('[data-trow^="day:"]')?.click());
    await browser.pause(700);
    await snap('newest day expanded (worked-on lines)');
    await browser.saveScreenshot(join(OUT_DIR, '02-L1-worked-on.png'));
    await browser.execute(() => document.querySelector('[data-trow^="day:"]')?.click());
    await browser.pause(500);

    // Thread click-to-filter: first thread filters and auto-expands member days.
    const hadThreads = await browser.execute(
      () => !!document.querySelector('[data-trow^="thread:"]'),
    );
    if (hadThreads) {
      await browser.execute(() => document.querySelector('[data-trow^="thread:"]')?.click());
      await browser.pause(900);
      await snap('thread filtered (member days auto-expanded to L1)');
      await browser.saveScreenshot(join(OUT_DIR, '03-thread-filtered.png'));
      await browser.execute(() => document.querySelector('[data-trow^="thread:"]')?.click());
      await browser.pause(500);
    }

    // Open-loops flatten mode with inline loops at L1.
    await browser.execute(() => {
      [...document.querySelectorAll('button')]
        .find((b) => b.textContent.trim().startsWith('Open loops'))
        ?.click();
    });
    await browser.pause(700);
    await snap('open-loops flatten mode');
    await browser.saveScreenshot(join(OUT_DIR, '04-open-loops.png'));

    writeFileSync(join(OUT_DIR, 'transcript.json'), JSON.stringify(transcript, null, 2));
  });
});
