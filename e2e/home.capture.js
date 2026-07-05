// Home-screen verification capture — NOT part of the golden-master suite
// (filename does not match *.e2e.js; run with --spec e2e/home.capture.js).
// Drives the new default Home view (first-screen-spec.md) end to end against
// the real list_threads command and saves PNGs + a DOM probe log.
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const OUT_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'qa', 'screenshots', 'home-verify');
mkdirSync(OUT_DIR, { recursive: true });

const SETTLE_MS = 700;
const notes = [];

async function shot(name) {
  await browser.pause(SETTLE_MS);
  await browser.saveScreenshot(join(OUT_DIR, name));
}

describe('home screen verification', () => {
  it('boots into Home and renders ranked threads end to end', async () => {
    await browser.waitUntil(async () => (await browser.getTitle()) === 'Claude Sessions', {
      timeout: 30000,
      timeoutMsg: 'frontend did not load',
    });
    // Wait for real data (list_threads must resolve or bootstrapping never ends).
    await browser.pause(2500);

    // ── Default view is Home: orientation line + hero present, no session list.
    const probe = await browser.execute(() => {
      const text = document.body.innerText;
      const heroResume = [...document.querySelectorAll('button')]
        .find((b) => b.textContent.trim() === 'Resume' && b.className.includes('bg-accent'));
      const compactResumes = [...document.querySelectorAll('button')]
        .filter((b) => b.textContent.trim() === 'Resume' && !b.className.includes('bg-accent'));
      const browseBtn = [...document.querySelectorAll('button')]
        .find((b) => /^Browse all \d+ sessions$/.test(b.textContent.trim()));
      return {
        orientation: /active this week\.|You were last here/.test(text),
        orientationText: (text.split('\n').find((l) => /active this week\.|You were last here/.test(l)) || '').trim(),
        heroName: document.querySelector('h2')?.textContent?.trim() ?? null,
        heroResume: Boolean(heroResume),
        forkBtn: [...document.querySelectorAll('button')].some((b) => b.textContent.trim() === 'fork'),
        leftOff: /Left off|When you left:/.test(text),
        compactResumeCount: compactResumes.length,
        browseAll: browseBtn ? browseBtn.textContent.trim() : null,
        searchInput: Boolean(document.querySelector('[placeholder="Search sessions"]')),
        // The flat launcher list must NOT be mounted on the first screen.
        launcherHeader: /^\d+ sessions?$/m.test(text),
        whySnippets: [...document.querySelectorAll('p, span')]
          .map((el) => el.textContent.trim())
          .filter((t) => /^(Last active|Active \d+ of the last 14 days|Last session|Pinned|Marked in progress)/.test(t))
          .slice(0, 5),
      };
    });
    notes.push({ step: '01-home-default', ...probe });
    await shot('01-home-default.png');

    expect(probe.orientation).toBe(true);
    expect(probe.heroName).not.toBeNull();
    expect(probe.heroResume).toBe(true);
    expect(probe.forkBtn).toBe(true);
    expect(probe.browseAll).not.toBeNull();
    expect(probe.launcherHeader).toBe(false);
    expect(probe.whySnippets.length).toBeGreaterThan(0);

    // ── Probe: exit-row search carries the query into the browse view.
    const exitSearch = await $('[placeholder="Search sessions"]');
    await exitSearch.setValue('refactor');
    await browser.keys('Enter');
    await browser.pause(SETTLE_MS);
    const afterSearch = await browser.execute(() => ({
      topSearchValue: document.querySelector('[placeholder="Search sessions & recaps…"]')?.value ?? null,
      hasSessionHeader: /\d+ sessions?/.test(document.body.innerText),
    }));
    notes.push({ step: '02-exit-search', ...afterSearch });
    await shot('02-browse-via-search.png');
    expect(afterSearch.topSearchValue).toBe('refactor');

    // ── Probe: Home tab returns; ranking identical (no refetch on view switch).
    await browser.execute(() => {
      [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Home')?.click();
    });
    await browser.pause(SETTLE_MS);
    const backHome = await browser.execute(() => ({
      heroName: document.querySelector('h2')?.textContent?.trim() ?? null,
    }));
    notes.push({ step: '03-back-home', ...backHome });
    await shot('03-back-home.png');
    expect(backHome.heroName).toBe(probe.heroName);

    // ── Probe: secondary row click opens browse (does NOT resume).
    const rowOpened = await browser.execute(() => {
      const row = [...document.querySelectorAll('div.cursor-pointer')]
        .find((d) => [...d.querySelectorAll('button')].some((b) => b.textContent.trim() === 'Resume'));
      if (!row) return { found: false };
      const name = row.querySelector('span')?.textContent?.trim() ?? null;
      row.click();
      return { found: true, name };
    });
    await browser.pause(SETTLE_MS);
    const afterRow = await browser.execute(() => ({
      inBrowse: Boolean(document.querySelector('[placeholder="Search sessions & recaps…"]')),
      sidebarSelected: document.querySelector('nav [aria-current], nav .bg-accent-soft')?.textContent?.trim() ?? null,
    }));
    notes.push({ step: '04-secondary-row', ...rowOpened, ...afterRow });
    await shot('04-row-opens-browse.png');
    if (rowOpened.found) expect(afterRow.inBrowse).toBe(true);

    writeFileSync(join(OUT_DIR, 'probe-log.json'), JSON.stringify(notes, null, 2));
  });
});
