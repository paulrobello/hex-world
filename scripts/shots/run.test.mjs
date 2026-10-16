import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const runner = fileURLToPath(new URL('./run.sh', import.meta.url));

function fixture(run) {
  const root = mkdtempSync(join(tmpdir(), 'hex-shots-'));
  const bin = join(root, 'bin');
  mkdirSync(bin);
  mkdirSync(join(root, 'target/shots'), { recursive: true });
  for (const [name, body] of Object.entries({
    caffeinate: 'printf "%s\\n" "$*" > awake; shift 2; exec "$@"',
    magick: 'printf "%s\\n" "${TEST_MEAN:-0.5}"',
    ffmpeg: 'printf "%s\\n" "$*" >> encoded',
  })) {
    writeFileSync(join(bin, name), `#!/bin/bash\n${body}\n`, { mode: 0o755 });
  }
  const invoke = (args, env = {}) => spawnSync('bash', [runner, ...args], {
    cwd: root,
    env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, ...env },
    encoding: 'utf8',
  });
  try { run({ root, invoke }); } finally { rmSync(root, { recursive: true, force: true }); }
}

test('capture keeps the display awake and rejects a black frame', () => fixture(({ root, invoke }) => {
  const command = ['bash', '-c', 'printf image > target/shots/frame.png'];
  assert.equal(invoke(command).status, 0);
  assert.match(readFileSync(join(root, 'awake'), 'utf8'), /^-d -u bash/);
  const black = invoke(command, { TEST_MEAN: '0.01' });
  assert.equal(black.status, 1);
  assert.match(black.stderr, /is black/);
}));

test('a failed capture command propagates its exit status', () => fixture(({ invoke }) => {
  assert.equal(invoke(['bash', '-c', 'exit 23']).status, 23);
}));

test('a successful command without frames is not capture evidence', () => fixture(({ invoke }) => {
  const result = invoke(['true']);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /no PNG frames/);
}));

test('JPEG batches validate every image before encoding any evidence', () => fixture(({ root, invoke }) => {
  writeFileSync(join(root, 'target/shots/one.png'), 'image');
  const missing = invoke(['--jpegs', 'evidence', 'one', 'two']);
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /missing/);
  writeFileSync(join(root, 'target/shots/two.png'), 'image');
  assert.equal(invoke(['--jpegs', 'evidence', 'one', 'two']).status, 0);
  assert.equal(readFileSync(join(root, 'encoded'), 'utf8').trim().split('\n').length, 2);
}));
