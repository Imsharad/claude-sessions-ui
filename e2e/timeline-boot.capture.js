// Diagnostic: what is expanded immediately after entering Timeline, and why.
describe('timeline boot state', () => {
  it('dumps disclosure state right after mount', async () => {
    await browser.waitUntil(async () => (await browser.getTitle()) === 'Claude Sessions', {
      timeout: 30000,
    });
    await browser.pause(1500);
    await browser.execute(() => {
      [...document.querySelectorAll('button')]
        .find((b) => b.textContent.trim() === 'Timeline')
        ?.click();
    });
    await browser.waitUntil(
      async () => await browser.execute(() => !!document.querySelector('[data-trow]')),
      { timeout: 20000 },
    );
    for (const wait of [0, 500, 1500]) {
      await browser.pause(wait);
      const state = await browser.execute(() => ({
        owners: [...document.querySelectorAll('[data-owner]')].map((r) =>
          r.getAttribute('data-owner'),
        ),
        ariaExpandedTrue: [...document.querySelectorAll('[aria-expanded="true"]')].map(
          (b) => b.getAttribute('data-trow') ?? b.textContent.slice(0, 40),
        ),
        activeEl:
          document.activeElement?.getAttribute?.('data-trow') ??
          document.activeElement?.tagName,
        scrollTop:
          [...document.querySelectorAll('.overflow-y-auto')].find((el) =>
            el.querySelector('[data-trow]'),
          )?.scrollTop ?? null,
      }));
      console.log(`BOOTSTATE +${wait}ms: ${JSON.stringify(state)}`);
    }
  });
});
