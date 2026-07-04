// Golden-master E2E: locks the observable behavior of the app across refactors.
// Anchored on real rendered structure (verified via DOM capture). Data-dependent checks
// degrade gracefully when the machine has no indexed sessions.

describe('app boot + shell', () => {
  it('launches with the correct document title', async () => {
    await browser.waitUntil(async () => (await browser.getTitle()) === 'Claude Sessions', {
      timeout: 30000,
      timeoutMsg: 'app never reported title "Claude Sessions" (frontend did not load)',
    });
    expect(await browser.getTitle()).toBe('Claude Sessions');
  });

  it('renders the top-bar controls (search + reindex)', async () => {
    await expect($('[placeholder="Search sessions & recaps…"]')).toBeExisting();
    await expect($('[title="Re-scan for new sessions"]')).toBeExisting();
  });

  it('renders the sidebar nav and the session-count heading', async () => {
    await expect($('nav')).toBeExisting();
    await expect($('h2')).toBeExisting();
  });
});

describe('session list', () => {
  it('renders session rows, or a clear empty state', async () => {
    const rows = await $$('[title="Pin to top"]'); // one pin control per rendered row
    if (rows.length === 0) {
      await expect($('*=No sessions match')).toBeExisting();
      return;
    }
    expect(rows.length).toBeGreaterThan(0);
  });
});

describe('search interaction', () => {
  it('accepts typed input', async () => {
    const search = await $('[placeholder="Search sessions & recaps…"]');
    await search.waitForDisplayed();
    await search.setValue('refactor');
    await expect(search).toHaveValue('refactor');
    await search.clearValue();
  });
});

describe('session selection', () => {
  it('clicking a row marks it active (accent styling)', async () => {
    const rows = await $$('div[class*="cursor-pointer"][class*="rounded-lg"]');
    if (rows.length < 2) return; // no data / single row — nothing meaningful to switch to
    const target = rows[1];
    await target.click();
    await browser.waitUntil(
      async () => ((await target.getAttribute('class')) || '').includes('accent'),
      { timeout: 10000, timeoutMsg: 'row did not gain selected (accent) styling after click' },
    );
  });
});

describe('hidden projects (blacklist)', () => {
  it('exposes a quiet control that reveals the manage panel', async () => {
    const control = await $('[title="Hidden projects"]');
    await control.waitForExist({ timeout: 10000 });
    await control.click();
    // Panel reveals: the add-pattern input is always present when open (and with
    // zero patterns the warm guidance line shows instead of a dead zone).
    await browser.waitUntil(
      async () => (await $('[placeholder="e.g. project-name/**"]')).isExisting(),
      { timeout: 10000, timeoutMsg: 'blacklist panel did not reveal its add-pattern input' },
    );
    await expect($('[placeholder="e.g. project-name/**"]')).toBeExisting();
  });
});

describe('backend', () => {
  it('has a live Tauri IPC bridge (Rust backend reachable)', async () => {
    const ok = await browser.execute(() => typeof window.__TAURI_INTERNALS__ !== 'undefined');
    expect(ok).toBe(true);
  });
});
