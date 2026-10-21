import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const runner = fileURLToPath(new URL('./run.sh', import.meta.url));

// A solid grey PNG (Pillow), so run.sh's real black-frame check reads it.
const pngCommand = (path, level) => [
  'python3',
  '-c',
  `from PIL import Image; Image.new("RGB", (64, 40), (${level},) * 3).save("${path}")`,
];

function fixture(run) {
  const root = mkdtempSync(join(tmpdir(), 'hex-shots-'));
  const bin = join(root, 'bin');
  mkdirSync(bin);
  mkdirSync(join(root, 'target/shots'), { recursive: true });
  for (const [name, body] of Object.entries({
    caffeinate: 'printf "%s\\n" "$*" > awake; shift 2; exec "$@"',
    // The frame check is Pillow's; any ImageMagick call fails the test.
    magick: 'echo "magick called" >&2; exit 99',
    ffmpeg: 'printf "%s\\n" "$*" >> encoded',
  })) {
    writeFileSync(join(bin, name), `#!/bin/bash\n${body}\n`, { mode: 0o755 });
  }
  const invoke = (args, env = {}) => spawnSync('bash', [runner, ...args], {
    cwd: root,
    env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, ...env },
    encoding: 'utf8',
  });
  const png = (name, level) => {
    const [cmd, ...args] = pngCommand(`target/shots/${name}.png`, level);
    const made = spawnSync(cmd, args, { cwd: root });
    assert.equal(made.status, 0, String(made.stderr));
  };
  try { run({ root, invoke, png }); } finally { rmSync(root, { recursive: true, force: true }); }
}

test('capture keeps the display awake and rejects a black frame', () => fixture(({ root, invoke }) => {
  const scene = invoke(pngCommand('target/shots/frame.png', 120));
  assert.equal(scene.status, 0, scene.stderr);
  assert.match(readFileSync(join(root, 'awake'), 'utf8'), /^-d -u python3/);
  const black = invoke(pngCommand('target/shots/frame.png', 3));
  assert.equal(black.status, 1);
  assert.match(black.stderr, /frame\.png is black \(mean 0\.01\d+\): idle display or high load/);
}));

test('a failed capture command propagates its exit status', () => fixture(({ invoke }) => {
  assert.equal(invoke(['bash', '-c', 'exit 23']).status, 23);
}));

test('a successful command without frames is not capture evidence', () => fixture(({ invoke }) => {
  const result = invoke(['true']);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /no PNG frames/);
  assert.equal(invoke(['--state-only', 'true']).status, 0);
}));

test('an empty or unreadable capture frame fails', () => fixture(({ invoke }) => {
  const empty = invoke(['bash', '-c', ': > target/shots/frame.png']);
  assert.equal(empty.status, 1);
  assert.match(empty.stderr, /frame\.png is empty/);
  const corrupt = invoke(['bash', '-c', 'printf image > target/shots/frame.png']);
  assert.equal(corrupt.status, 1);
  assert.match(corrupt.stderr, /frame\.png is unreadable/);
}));

test('JPEG batches validate every image before encoding any evidence', () => fixture(({ root, invoke, png }) => {
  png('one', 120);
  const missing = invoke(['--jpegs', 'evidence', 'one', 'two']);
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /two\.png is missing/);
  png('two', 0);
  const black = invoke(['--jpegs', 'evidence', 'one', 'two']);
  assert.equal(black.status, 1);
  assert.match(black.stderr, /two\.png is black/);
  assert.throws(() => readFileSync(join(root, 'encoded')));
  png('two', 120);
  const ok = invoke(['--jpegs', 'evidence', 'one', 'two']);
  assert.equal(ok.status, 0, ok.stderr);
  assert.equal(readFileSync(join(root, 'encoded'), 'utf8').trim().split('\n').length, 2);
}));
