// node --test scripts/assets/kit-lib.test.mjs (run by make ui-test, so CI runs it). Node builtins and kit-lib only.
import assert from "node:assert/strict";
import { test } from "node:test";
import { encodeSettingsDrift, kitItemDir, ktxPlatformError } from "./kit-lib.mjs";

const manifest = {
  version: 1,
  materials: {
    grass: { maps: { albedo: "p1/materials/grass/grass.albedo", normal: "p1/materials/grass/grass.normal" } },
    soil: { maps: { albedo: "p1/materials/soil/soil.albedo" } },
  },
  pieces: [
    { id: "meadow_base", glb: "p1/meadow_base/meadow_base.glb", maps: { masks: "p1/meadow_base/meadow_base.masks" } },
    { id: "road", glb: "p1/road/road.glb" },
    { id: "moved", glb: "p2/moved/moved.glb", maps: { albedo: "p2/moved/moved.albedo" } },
    { id: "odd", glb: "p3/elsewhere/odd.glb" },
  ],
};

test("a listed piece or set lives where its manifest entry says", () => {
  assert.equal(kitItemDir(manifest, "piece", "meadow_base"), "p1/meadow_base");
  assert.equal(kitItemDir(manifest, "piece", "moved"), "p2/moved");
  assert.equal(kitItemDir(manifest, "piece", "odd"), "p3/elsewhere");
  assert.equal(kitItemDir(manifest, "material", "grass"), "p1/materials/grass");
});

test("an unlisted piece or set takes the folder its siblings share", () => {
  const m = { ...manifest, pieces: manifest.pieces.filter((p) => p.id !== "moved") };
  assert.equal(kitItemDir(m, "piece", "new_piece"), "p1/new_piece");
  assert.equal(kitItemDir(m, "material", "bark"), "p1/materials/bark");
});

test("a layout that cannot be derived is a clear error naming --out", () => {
  // Siblings in two parent folders: no single convention to follow.
  assert.throws(() => kitItemDir(manifest, "piece", "new_piece"),
    /new_piece: cannot derive its output folder from the manifest .*p1, p2.*--out/);
  assert.throws(() => kitItemDir({ version: 1, pieces: [], materials: {} }, "piece", "first"),
    /first: cannot derive its output folder from the manifest .*no piece to follow.*--out/);
  // One entry whose files sit in two folders.
  const split = { version: 1, materials: {}, pieces: [{ id: "x", glb: "p1/x/x.glb", maps: { masks: "p1/y/x.masks" } }] };
  assert.throws(() => kitItemDir(split, "piece", "x"), /x: manifest lists its files in more than one folder \(p1\/x, p1\/y\)/);
  assert.throws(() => kitItemDir(manifest, "prop", "x"), /unknown kind prop/);
});

test("kit-compress states its platform limit up front on any host but macOS arm64", () => {
  assert.equal(ktxPlatformError("darwin", "arm64"), null);
  for (const [platform, arch] of [["linux", "x64"], ["darwin", "x64"], ["win32", "arm64"], ["linux", "arm64"]]) {
    const e = ktxPlatformError(platform, arch);
    assert.match(e, new RegExp(`macOS arm64 only .*this host is ${platform} ${arch}`));
    assert.match(e, /Apple Silicon Mac/);
  }
});

test("encodeSettingsDrift names each encoder setting a re-encode changes, and the legacy encodes by commit", () => {
  const now = { toolVersions: { ktxSoftware: "ktx version: v4.4.2" },
    settings: { uastcQuality: 4, uastcThreads: 1, uastcAslrOff: true, zstd: 18, rdo: { normal: { srgb: false, rdo: 0.25 } } } };
  assert.deepEqual(encodeSettingsDrift(now, structuredClone(now)), []);
  // Key order inside a setting is not a change.
  const reordered = structuredClone(now);
  reordered.settings.rdo = { normal: { rdo: 0.25, srgb: false } };
  assert.deepEqual(encodeSettingsDrift(reordered, now), []);
  // The core material sets' committed reports (2026-10-16): neither the thread pin nor the ASLR-off spawn.
  const legacy = structuredClone(now);
  delete legacy.settings.uastcThreads;
  delete legacy.settings.uastcAslrOff;
  assert.deepEqual(encodeSettingsDrift(legacy, now), [
    "settings.uastcAslrOff: absent (encoded before 6e96b12, 2026-10-26, with ASLR on) -> true",
    "settings.uastcThreads: absent (encoded before b6cd857, 2026-10-21, with the RDO pass split across up to 4 threads) -> 1",
  ]);
  const other = structuredClone(now);
  other.settings.zstd = 20;
  other.toolVersions.ktxSoftware = "ktx version: v5.0.0";
  assert.deepEqual(encodeSettingsDrift(now, other),
    ["settings.zstd: 18 -> 20", "toolVersions.ktxSoftware: ktx version: v4.4.2 -> ktx version: v5.0.0"]);
  // No committed report (a first compress): every current setting is new, which the caller skips.
  assert.equal(encodeSettingsDrift(undefined, now).length, 6);
});
