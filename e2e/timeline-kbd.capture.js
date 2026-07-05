// Timeline keyboard walkthrough — NOT part of the golden-master suite (filename
// does not match the *.e2e.js glob; run explicitly with --spec). Drives the new
// progressive-disclosure timeline with real key presses and records, after each
// keystroke, where focus is and what is expanded. Evidence lands in
// qa/screenshots/timeline-kbd/ (transcript.json + PNGs).
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const OUT_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'qa', 'screenshots', 'timeline-kbd');
mkdirSync(OUT_DIR, { recursive: true });

const transcript = [];

async function snap(step, extra = {}) {
  const state = await browser.execute(() => {
    const ae = document.activeElement;
    const body = [...document.querySelectorAll('.overflow-y-auto')].find((el) =>
      el.querySelector('[data-trow]'),
    );
    return {
      focusedRow: ae?.getAttribute?.('data-trow') ?? `<${ae?.tagName?.toLowerCase() ?? 'none'}>`,
      focusedAriaExpanded: ae?.getAttribute?.('aria-expanded') ?? null,
      expandedRegions: [...document.querySelectorAll('[data-owner]')].map((r) =>
        r.getAttribute('data-owner'),
      ),
      firstRowTop: Math.round(
        document.querySelector('[data-trow]')?.getBoundingClientRect().top ?? -1,
      ),
      l0Fits: body ? body.scrollHeight <= body.clientHeight + 1 : null,
      dayRows: document.querySelectorAll('[data-trow^="day:"]').length,
      threadRows: document.querySelectorAll('[data-trow^="thread:"]').length,
      cardVisible: !!document.querySelector('[data-owner^="sess:"] [class*="rounded-lg"]'),
    };
  });
  transcript.push({ step, ...state, ...extra });
}

describe('timeline keyboard walkthrough', () => {
  it('roving focus, expand, collapse-returns-focus', async () => {
    await browser.waitUntil(async () => (await browser.getTitle()) === 'Claude Sessions', {
      timeout: 30000,
      timeoutMsg: 'frontend did not load',
    });
    await browser.pause(1500);

    // Enter the Timeline view via the top bar.
    await browser.execute(() => {
      [...document.querySelectorAll('button')]
        .find((b) => b.textContent.trim() === 'Timeline')
        ?.click();
    });
    await browser.waitUntil(
      async () => await browser.execute(() => !!document.querySelector('[data-trow]')),
      { timeout: 20000, timeoutMsg: 'timeline rows never rendered' },
    );
    await browser.pause(800);
    await snap('L0 default render (no interaction yet)');
    await browser.saveScreenshot(join(OUT_DIR, '01-L0-default.png'));

    // Tab-stop entry: focus the single roving tab stop (tabindex=0 row).
    await browser.execute(() => document.querySelector('[data-trow][tabindex="0"]')?.focus());
    await snap('focus roving tab stop', { keys: '(Tab lands here — one stop for all rows)' });

    // ArrowDown until a day row has focus (walks threads first, if any).
    let downs = 0;
    for (; downs < 20; downs++) {
      const onDay = await browser.execute(() =>
        document.activeElement?.getAttribute('data-trow')?.startsWith('day:'),
      );
      if (onDay) break;
      await browser.keys('ArrowDown');
      await snap('ArrowDown', { keys: 'ArrowDown' });
    }

    // Enter expands the focused day (L1). Content above must not move.
    const topBefore = transcript[transcript.length - 1].firstRowTop;
    await browser.keys('Enter');
    await browser.pause(400);
    await snap('Enter on day row -> expands to L1', { keys: 'Enter', firstRowTopBefore: topBefore });
    await browser.saveScreenshot(join(OUT_DIR, '02-L1-day-expanded.png'));

    // ArrowDown moves into the first session row of the expanded day.
    await browser.keys('ArrowDown');
    await snap('ArrowDown -> first session row (L1)', { keys: 'ArrowDown' });

    // Enter expands the session to the full digest card (L2).
    await browser.keys('Enter');
    await browser.pause(400);
    await snap('Enter on session row -> expands to L2 card', { keys: 'Enter' });
    await browser.saveScreenshot(join(OUT_DIR, '03-L2-session-card.png'));

    // Escape collapses the session and keeps focus on the owning row.
    await browser.keys('Escape');
    await browser.pause(300);
    await snap('Escape -> collapses L2, focus stays on session row', { keys: 'Escape' });

    // ArrowLeft on a collapsed session row returns focus to the owning day row.
    await browser.keys('ArrowLeft');
    await snap('ArrowLeft on collapsed session -> focus owner day row', { keys: 'ArrowLeft' });

    // ArrowLeft on the expanded day collapses it; focus stays on the day row.
    await browser.keys('ArrowLeft');
    await browser.pause(300);
    await snap('ArrowLeft on day row -> collapses L1, focus stays', { keys: 'ArrowLeft' });

    // ArrowUp walks back up the visible rows.
    await browser.keys('ArrowUp');
    await snap('ArrowUp -> previous visible row', { keys: 'ArrowUp' });

    writeFileSync(join(OUT_DIR, 'transcript.json'), JSON.stringify(transcript, null, 2));
  });
});
