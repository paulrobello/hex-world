// Sends a world-building script and waits until all of it has run, then checks the tile count.
// Builds before the bridge's `hex_world_inbox_len` (perf runs compare older dists) report
// `state().queued` as of the last frame, so straight after a send it reads the old value, often 0,
// until the next frame drains the inbox. The wait also needs a state published after the send.
export async function buildWorld(page, script, tiles, timeout = 600_000) {
  const sentAt = await page.evaluate((s) => {
    const f = window.hexWorld.state().frames;
    window.hexWorld.cmd(s);
    return f;
  }, script);
  await page.waitForFunction((f) => {
    const s = window.hexWorld.state();
    return s.frames > f && s.queued === 0 && !s.play?.rising?.length;
  }, sentAt, { timeout, polling: 500 });
  const n = await page.evaluate(() => window.hexWorld.state().play?.tiles?.length);
  if (n !== tiles) throw new Error(`world has ${n} tiles, expected ${tiles}`);
}
