// node --test scripts/assets/kit-provenance.test.mjs (run by make kit-compress-test). Node builtins only.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { assembleProvenance } from "./kit-provenance.mjs";
import { sha256 } from "./kit-lib.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const CLI = join(dirname(fileURLToPath(import.meta.url)), "kit-provenance.mjs");
const cli = (opts, ...args) => spawnSync(process.execPath, [CLI, opts.id, "--source", opts.source, "--runtime", opts.runtime,
  "--refs", opts.refs, ...args], { encoding: "utf8" });

async function stage({ approval = true, maskTransfer = "linear", inManifest = true } = {}) {
  const dir = await mkdtemp(join(tmpdir(), "kitprov-"));
  const src = join(dir, "source"), runtime = join(dir, "runtime"), refs = join(dir, "refs");
  await mkdir(join(runtime, "p1/pc"), { recursive: true });
  await mkdir(src, { recursive: true });
  await mkdir(refs, { recursive: true });
  await writeFile(join(runtime, "p1/pc/pc.glb"), "glb-bytes");
  await writeFile(join(runtime, "manifest.json"), JSON.stringify({ version: 1, materials: {}, pieces: inManifest ? [{ id: "pc" }] : [] }));
  const masks = join(runtime, "p1/pc/pc.masks.png");
  await writeFile(masks, "png-bytes");
  const w = (p, v) => writeFile(p, JSON.stringify(v));
  const gen = join(dir, "pc.py"), helper = join(dir, "_pc_helper.py");
  await writeFile(gen, "generator-bytes");
  await writeFile(helper, "helper-bytes");
  await w(join(src, "export-report.json"), { pieceId: "pc", nodes: ["pc"], lodTris: { pc_lod1: 50, pc_lod0: 100, pc_lod2: 10 },
    sockets: [], generator: { path: gen, sha256: sha256(Buffer.from("generator-bytes")) },
    generatorDeps: [{ path: helper, sha256: sha256(Buffer.from("helper-bytes")) }], blender: "5.2.2" });
  await w(join(src, "bake-report.json"), { device: "CPU", receiverSignature: "sig" });
  await w(join(src, "compress-report.json"), { toolVersions: { node: "v1" }, settings: { meshopt: "QUANTIZE" },
    budgets: { class: "piece", ktxMax: 1_500_000, ktxBytes: { bc7: 0, astc: 0 } },
    textureEncodes: [{ channel: "masks", family: "png", path: masks, format: "PNG", transfer: maskTransfer, bytes: 9, sha256: sha256(Buffer.from("png-bytes")) }] });
  await w(join(refs, "refs.json"), [{ path: "assets-source/refs/pc/ref-1.jpg", sha256: "11", sourceSha256: "22", provider: "higgsfield", prompt: "p" }]);
  if (approval) await w(join(refs, "approval.json"), { who: "owner", when: "2026-10-13", gate: "reference", approved: ["assets-source/refs/pc/ref-1.jpg"] });
  await w(join(src, "approvals.json"), [{ who: "owner", when: "2026-10-14", gate: "lookdev" }]);
  return { dir, gen, helper, opts: { id: "pc", source: src, runtime, refs, validated: "a,b" } };
}

test("reference approval reaches provenance ahead of the piece approvals (S5)", async () => {
  const { dir, opts } = await stage();
  try {
    const p = await assembleProvenance(opts);
    assert.equal(p.approvals[0].gate, "reference");
    assert.equal(p.approvals[1].gate, "lookdev");
    assert.equal(p.references[0].sourceSha256, "22");
    assert.deepEqual(p.lods.map((l) => l.lod), [0, 1, 2]);
    assert.equal(p.lods[1].ratio, 0.5);
    assert.equal(p.compression.budgets.class, "piece");
    assert.equal(p.derivedFiles[0].sha256, sha256(Buffer.from("glb-bytes")));
    assert.equal(p.validation.pass, true);
    assert.deepEqual(p.validation.checks, ["a", "b"]);
    const onDisk = JSON.parse(await readFile(join(opts.source, "provenance.json"), "utf8"));
    assert.deepEqual(onDisk.approvals, p.approvals);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("interactive route: blendRef is store-relative and the export stage stays hashed", async () => {
  const { dir, opts } = await stage();
  try {
    const store = join(dir, "store");
    await mkdir(join(store, "pc"), { recursive: true });
    await writeFile(join(store, "pc/pc_high.blend"), "blend-bytes");
    const p = await assembleProvenance({ ...opts, route: "interactive", blend: join(store, "pc/pc_high.blend"), blendStore: store });
    assert.equal(p.route, "interactive");
    assert.deepEqual(p.blendRef, { store: "local:assets-source/blend", key: "pc/pc_high.blend",
      sha256: sha256(Buffer.from("blend-bytes")), bytes: 11 });
    assert.equal(p.generator.path, join(dir, "pc.py"));
    assert.equal(p.generatorDeps.length, 1);
    await assert.rejects(assembleProvenance({ ...opts, blend: join(dir, "elsewhere.blend"), blendStore: store }),
      /outside the blend store/);
    // A store path that failed to resolve must not yield a blendRef-less interactive record.
    await assert.rejects(assembleProvenance({ ...opts, route: "interactive" }), /route interactive needs --blend/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("generator and helpers reach provenance as built, and stale or missing records are refused", async () => {
  const { dir, gen, helper, opts } = await stage();
  try {
    const rep = join(opts.source, "export-report.json");
    const exp = JSON.parse(await readFile(rep, "utf8"));
    assert.deepEqual((await assembleProvenance(opts)).generatorDeps, exp.generatorDeps);
    // A report from before generatorDeps existed is refused: the build must record what it imported.
    const { generatorDeps, ...old } = exp;
    await writeFile(rep, JSON.stringify(old));
    await assert.rejects(assembleProvenance(opts), /lacks generatorDeps/);
    await writeFile(rep, JSON.stringify(exp));
    // A helper or the generator edited after the build: rebuild before stamping provenance.
    await writeFile(helper, "edited");
    await assert.rejects(assembleProvenance(opts), /_pc_helper\.py changed since the build/);
    await writeFile(helper, "helper-bytes");
    await writeFile(gen, "edited");
    await assert.rejects(assembleProvenance(opts), /pc\.py changed since the build/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("refs without an owner approval are refused (R2)", async () => {
  const { dir, opts } = await stage({ approval: false });
  try {
    await assert.rejects(assembleProvenance(opts), /approval\.json/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("a linear channel PNG recorded as sRGB is refused", async () => {
  const { dir, opts } = await stage({ maskTransfer: "srgb" });
  try {
    await assert.rejects(assembleProvenance(opts), /masks png transfer srgb/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("validation.pass is refused for a piece the gate cannot see (not in the runtime manifest)", async () => {
  const { dir, opts } = await stage({ inManifest: false });
  try {
    await assert.rejects(assembleProvenance(opts), /not in .*manifest\.json/);
    const p = await assembleProvenance({ ...opts, validated: undefined });
    assert.equal(p.validation.pass, false);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

// Material mode (B3): a tileable set's provenance comes from the same assembler.
async function stageMaterial({ inManifest = true, albedoTransfer = "srgb" } = {}) {
  const dir = await mkdtemp(join(tmpdir(), "kitprov-mat-"));
  const src = join(dir, "source"), runtime = join(dir, "runtime");
  await mkdir(join(runtime, "p1/materials/ts"), { recursive: true });
  await mkdir(src, { recursive: true });
  const gen = join(dir, "mat_ts.py");
  await writeFile(gen, "generator-bytes");
  await writeFile(join(runtime, "manifest.json"), JSON.stringify({ version: 1, pieces: [],
    materials: inManifest ? { ts: { tileM: 2.0, maps: { albedo: "p1/materials/ts/ts.albedo" } } } : {} }));
  const encodes = [];
  for (const [family, transfer, vkFormat] of [["bc7", albedoTransfer, 146], ["astc", albedoTransfer, 158], ["png", albedoTransfer]]) {
    const path = join(runtime, `p1/materials/ts/ts.albedo.${family === "png" ? "png" : `${family}.ktx2`}`);
    await writeFile(path, `${family}-bytes`);
    encodes.push({ channel: "albedo", family, path, transfer, vkFormat, bytes: 9, sha256: sha256(Buffer.from(`${family}-bytes`)) });
  }
  const w = (p, v) => writeFile(p, JSON.stringify(v));
  await w(join(src, "bake-report.json"), { generator: gen, generatorSha256: sha256(Buffer.from("generator-bytes")),
    generatorDeps: [], set: "ts", size: 1024, tileM: 2.0, device: "CPU", samples: 32, blender: "5.2.2" });
  await w(join(src, "compress-report.json"), { toolVersions: { node: "v1" }, settings: { meshopt: "-" },
    budgets: { class: "tileable", ktxMax: 3_600_000, ktxBytes: { bc7: 9, astc: 9 } }, textureEncodes: encodes });
  return { dir, gen, opts: { id: "ts", material: true, source: src, runtime, validated: "a" } };
}

test("material mode hashes the generator, reports and every shipped file", async () => {
  const { dir, gen, opts } = await stageMaterial();
  try {
    const p = await assembleProvenance(opts);
    assert.equal(p.setId, "ts");
    assert.equal(p.pieceId, undefined);
    assert.equal(p.kind, "tileable");
    assert.equal(p.generator.sha256, sha256(Buffer.from("generator-bytes")), "hash as built");
    assert.deepEqual(p.generatorDeps, []);
    assert.equal(p.generator.path, relative(ROOT, gen), "generator path is recorded repo-relative");
    assert.equal(p.bake.tileM, 2.0);
    assert.equal(p.bakeReport.sha256.length, 64);
    assert.equal(p.compressReport.sha256.length, 64);
    assert.equal(p.compression.budgets.class, "tileable");
    assert.deepEqual(p.derivedFiles.map((f) => f.path.split("/").pop()).sort(),
      ["ts.albedo.astc.ktx2", "ts.albedo.bc7.ktx2", "ts.albedo.png"]);
    assert.equal(p.derivedFiles.find((f) => f.path.endsWith(".png")).sha256, sha256(Buffer.from("png-bytes")));
    assert.equal(p.authoringAgent.model, "claude-opus-5-5");
    assert.equal(p.validation.pass, true);
    const onDisk = JSON.parse(await readFile(join(opts.source, "provenance.json"), "utf8"));
    assert.deepEqual(onDisk, p);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("material mode refuses a generator edited after the bake", async () => {
  const { dir, gen, opts } = await stageMaterial();
  try {
    await writeFile(gen, "edited");
    await assert.rejects(assembleProvenance(opts), /mat_ts\.py changed since the build/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("material mode refuses validation for a set the manifest does not list", async () => {
  const { dir, opts } = await stageMaterial({ inManifest: false });
  try {
    await assert.rejects(assembleProvenance(opts), /ts: not in .*manifest\.json materials/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("material mode refuses an albedo recorded as linear (R6)", async () => {
  const { dir, opts } = await stageMaterial({ albedoTransfer: "linear" });
  try {
    await assert.rejects(assembleProvenance(opts), /albedo bc7 transfer linear, expected srgb/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

// make kit-provenance once ran with no target/kit-stage/<id>/: kit_render.py failed, but the first
// kit-provenance.mjs call had already replaced a passing provenance.json with validation.pass false.
test("a missing build stage is refused before provenance.json is touched", async () => {
  const { dir, opts } = await stage();
  try {
    const out = join(opts.source, "provenance.json");
    await writeFile(out, JSON.stringify({ pieceId: "pc", validation: { pass: true } }) + "\n");
    const before = await readFile(out);
    const st = join(dir, "kit-stage/pc");
    await assert.rejects(assembleProvenance({ ...opts, stage: st }),
      (e) => e.message.includes(`build stage ${st} has no pc.glb`) && e.message.includes("run make kit-build PIECE=pc first"));
    await mkdir(st, { recursive: true });
    await writeFile(join(st, "pc.glb"), "glb-bytes");
    await assert.rejects(assembleProvenance({ ...opts, stage: st }), /has no export-report\.json; run make kit-build PIECE=pc/);
    assert.deepEqual(await readFile(out), before);
    // The Makefile's calls, check and validated: one line on stderr, a non-zero exit, no write.
    const missing = join(dir, "kit-stage/none");
    for (const args of [["--check"], ["--validated", "a,b"]]) {
      const r = cli(opts, "--stage", missing, ...args);
      assert.equal(r.status, 1, r.stderr);
      assert.equal(r.stderr.trim(), `kit-provenance: pc: build stage ${missing} has no pc.glb; run make kit-build PIECE=pc first`);
      assert.deepEqual(await readFile(out), before);
    }
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("--check verifies everything and writes nothing; only the validated run writes", async () => {
  const { dir, gen, opts } = await stage();
  try {
    const out = join(opts.source, "provenance.json");
    await writeFile(out, "committed\n");
    const before = await readFile(out);
    const st = join(dir, "kit-stage/pc");
    await mkdir(st, { recursive: true });
    await writeFile(join(st, "pc.glb"), "glb-bytes");
    await writeFile(join(st, "export-report.json"), "{}");
    assert.equal((await assembleProvenance({ ...opts, stage: st, check: true })).validation.pass, true);
    assert.deepEqual(await readFile(out), before);
    const ok = cli(opts, "--stage", st, "--check");
    assert.equal(ok.status, 0, ok.stderr);
    assert.match(ok.stdout, /^checked pc: .*provenance\.json not written/);
    assert.deepEqual(await readFile(out), before);
    // A failed check (a generator edited after the build) exits non-zero and leaves the file alone too.
    await writeFile(gen, "edited");
    const stale = cli(opts, "--stage", st, "--check");
    assert.equal(stale.status, 1);
    assert.match(stale.stderr, /^kit-provenance: pc: .*pc\.py changed since the build/);
    assert.deepEqual(await readFile(out), before);
    await writeFile(gen, "generator-bytes");
    const done = cli(opts, "--stage", st, "--validated", "a,b");
    assert.equal(done.status, 0, done.stderr);
    assert.equal(JSON.parse(await readFile(out, "utf8")).validation.pass, true);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
