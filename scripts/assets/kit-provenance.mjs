// Assembles assets-source/kit/<id>/provenance.json from stage reports (brief §(d), PRD §11.4).
// Usage: node scripts/assets/kit-provenance.mjs <id> [--source dir] [--runtime dir] [--route scripted|interactive]
//        [--refs assets-source/refs/<ref-id>] [--blend path.blend [--blend-store dir]] [--stage dir]
//        [--validated "<check list>"] [--check]
//        node scripts/assets/kit-provenance.mjs <set> --material [--source dir] [--runtime dir] [--validated "<list>"] [--check]
// Material mode (tileable sets) writes assets-source/kit/materials/<set>/provenance.json.
// --check assembles and verifies everything but writes nothing; --stage refuses a missing build stage.
// Paths resolve against the repo root, so absolute paths work too. Imports node builtins and kit-lib only.
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { CHANNELS, kitItemDir, sha256 } from "./kit-lib.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const read = async (p) => JSON.parse(await readFile(p, "utf8"));
const file = async (p) => {
  const b = await readFile(p);
  return { path: relative(ROOT, p), sha256: sha256(b), bytes: b.length };
};

// R6: the PNG fallback carries no color space, so the recorded transfer is the only record of how
// the runtime must load it (normal/orm/masks linear, is_srgb = false).
function checkTransfers(id, encodes) {
  for (const t of encodes) {
    const want = CHANNELS[t.channel]?.srgb ? "srgb" : "linear";
    if (!CHANNELS[t.channel] || t.transfer !== want)
      throw new Error(`${id}: ${t.channel} ${t.family} transfer ${t.transfer}, expected ${want} (R6)`);
  }
}

// The build recorded the generator and helper hashes it ran. Refuse to stamp provenance when either
// changed since, so validation.pass never covers source the shipped files did not come from.
async function checkSources(id, generator, deps) {
  if (!generator?.path || !generator?.sha256) throw new Error(`${id}: build report lacks generator {path, sha256}`);
  if (!Array.isArray(deps))
    throw new Error(`${id}: build report lacks generatorDeps (built before helper hashing); rebuild with make kit-build / kit-material`);
  for (const f of [generator, ...deps]) {
    const now = sha256(await readFile(resolve(ROOT, f.path)));
    if (now !== f.sha256) throw new Error(`${id}: ${f.path} changed since the build (${f.sha256.slice(0, 12)} -> ${now.slice(0, 12)}); rebuild first`);
  }
}

// kit_render.py re-imports the staged GLB against the staged export report, so both must exist
// before make kit-provenance writes anything (a missing stage once replaced a passing record).
function checkStage(id, stage) {
  const dir = resolve(ROOT, stage);
  const shown = dir.startsWith(ROOT + sep) ? relative(ROOT, dir) : dir;
  for (const f of [`${id}.glb`, "export-report.json"])
    if (!existsSync(join(dir, f)))
      throw new Error(`${id}: build stage ${shown} has no ${f}; run make kit-build PIECE=${id} first`);
}

// A tileable set: generator and helper hashes as built, bake and compress reports, and every
// shipped encode. The set belongs to no piece, so it carries no references or approvals.
async function assembleMaterial({ id, source, runtime, validated, check }) {
  const src = resolve(ROOT, source ?? `assets-source/kit/materials/${id}`);
  const rt = resolve(ROOT, runtime ?? "assets/kit");
  if (validated && !((await read(join(rt, "manifest.json"))).materials ?? {})[id])
    throw new Error(`${id}: not in ${relative(ROOT, join(rt, "manifest.json"))} materials; add it before stamping validation.pass`);
  const bk = await read(join(src, "bake-report.json"));
  const cmp = await read(join(src, "compress-report.json"));
  checkTransfers(id, cmp.textureEncodes);
  const generator = { path: relative(ROOT, resolve(ROOT, bk.generator)), sha256: bk.generatorSha256 };
  await checkSources(id, generator, bk.generatorDeps);
  const derived = [];
  for (const t of cmp.textureEncodes) derived.push(await file(resolve(ROOT, t.path)));
  const prov = {
    setId: id, kind: "tileable", version: 1, route: "scripted",
    generator, generatorDeps: bk.generatorDeps,
    authoringAgent: { harness: "claude-code", model: "claude-opus-5-5" },
    blender: bk.blender, toolVersions: cmp.toolVersions, bake: bk,
    bakeReport: await file(join(src, "bake-report.json")),
    compressReport: await file(join(src, "compress-report.json")),
    compression: { budgets: cmp.budgets, textureEncodes: cmp.textureEncodes },
    derivedFiles: derived, license: "MIT (original work, Hex World repo)",
    validation: validated ? { pass: true, date: new Date().toISOString(), checks: validated.split(",") } : { pass: false },
  };
  if (!check) await writeFile(join(src, "provenance.json"), JSON.stringify(prov, null, 2) + "\n");
  return prov;
}

export async function assembleProvenance({ id, material = false, source, runtime, route = "scripted", refs, blend, blendStore, validated,
  stage, check = false }) {
  if (material) return assembleMaterial({ id, source, runtime, validated, check });
  if (stage) checkStage(id, stage);
  const src = resolve(ROOT, source ?? `assets-source/kit/${id}`);
  const rt = resolve(ROOT, runtime ?? "assets/kit");
  // validation.pass is only meaningful once kit_gate can see the piece, i.e. once it is in the manifest.
  if (validated && !(await read(join(rt, "manifest.json"))).pieces.some((p) => p.id === id))
    throw new Error(`${id}: not in ${relative(ROOT, join(rt, "manifest.json"))}; add it before stamping validation.pass`);
  const exp = await read(join(src, "export-report.json"));
  await checkSources(id, exp.generator, exp.generatorDeps);
  // R1: an interactive piece without its .blend reference is refused, never written.
  if (route === "interactive" && !blend) throw new Error(`${id}: route interactive needs --blend (the high-poly in the blend store)`);
  const bk = await read(join(src, "bake-report.json"));
  // A HEXW_SKIP_BAKE=1 build is a draft. Pieces with no Cycles bake by design say why in `reason`.
  if (bk.skipped === true && !String(bk.reason ?? "").trim())
    throw new Error(`${id}: bake report is skipped with no reason (a HEXW_SKIP_BAKE=1 draft); rebuild without HEXW_SKIP_BAKE, then kit-compress`);
  const cmp = await read(join(src, "compress-report.json"));
  checkTransfers(id, cmp.textureEncodes);
  // The GLB the manifest ships (or, before the entry exists, where its siblings' layout puts it).
  const manifest = await read(join(rt, "manifest.json"));
  const glb = manifest.pieces.find((p) => p.id === id)?.glb ?? `${kitItemDir(manifest, "piece", id)}/${id}.glb`;
  const derived = [await file(join(rt, glb))];
  for (const t of cmp.textureEncodes) derived.push(await file(resolve(ROOT, t.path)));
  let references = [];
  const approvals = [];
  if (refs) {
    const dir = resolve(ROOT, refs);
    references = await read(join(dir, "refs.json"));
    // R2/S5: the owner's reference approval travels with the piece.
    if (!existsSync(join(dir, "approval.json")))
      throw new Error(`${id}: ${relative(ROOT, dir)}/approval.json missing; references are not owner-approved (R2)`);
    approvals.push(await read(join(dir, "approval.json")));
  }
  if (existsSync(join(src, "approvals.json"))) approvals.push(...(await read(join(src, "approvals.json"))));
  const lod = Object.entries(exp.lodTris)
    .map(([name, tris]) => ({ lod: Number(name.slice(-1)), tris }))
    .sort((a, b) => a.lod - b.lod);
  // Interactive pieces keep the export stage's generator hashes too: the stage is scripted.
  let blendRef;
  if (blend) {
    const abs = resolve(ROOT, blend);
    const store = resolve(ROOT, blendStore ?? dirname(dirname(abs)));
    const key = relative(store, abs);
    if (key.startsWith("..") || isAbsolute(key)) throw new Error(`${abs} is outside the blend store ${store}`);
    const { sha256: digest, bytes } = await file(abs);  // no host path: the key locates it in the store
    blendRef = { store: "local:assets-source/blend", key: key.split(sep).join("/"), sha256: digest, bytes };
  }
  const prov = {
    pieceId: id, version: 1, route,
    ...(blendRef ? { blendRef } : {}),
    generator: exp.generator, generatorDeps: exp.generatorDeps,
    authoringAgent: { harness: "claude-code", model: "claude-opus-5-5" },
    blender: exp.blender, toolVersions: cmp.toolVersions, bake: bk,
    lods: lod.map((l) => ({ ...l, ratio: l.tris / lod[0].tris })),
    collider: { node: `${id}_collider` }, sockets: exp.sockets, pivot: "hex_center",
    texelDensityPxPerM: bk.texelDensityPxPerM ?? null,
    compression: { meshopt: cmp.settings.meshopt, budgets: cmp.budgets, textureEncodes: cmp.textureEncodes },
    derivedFiles: derived, references, license: "MIT (original work, Hex World repo)",
    validation: validated ? { pass: true, date: new Date().toISOString(), checks: validated.split(",") } : { pass: false },
    approvals,
  };
  if (!check) await writeFile(join(src, "provenance.json"), JSON.stringify(prov, null, 2) + "\n");
  return prov;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [id, ...rest] = process.argv.slice(2);
  const opt = (k) => { const i = rest.indexOf(`--${k}`); return i >= 0 ? rest[i + 1] : undefined; };
  if (!id || id.startsWith("--")) {
    console.error("usage: kit-provenance.mjs <id> [--material] [--source d --runtime d --route r --refs d --blend f --blend-store d --stage d --validated list --check]");
    process.exit(2);
  }
  const material = rest.includes("--material"), check = rest.includes("--check");
  const source = opt("source") ?? (material ? `assets-source/kit/materials/${id}` : `assets-source/kit/${id}`);
  const out = relative(ROOT, resolve(ROOT, source, "provenance.json"));
  try {
    const prov = await assembleProvenance({ id, material, source, runtime: opt("runtime"), route: opt("route"), blendStore: opt("blend-store"),
      refs: opt("refs"), blend: opt("blend"), stage: opt("stage"), validated: opt("validated"), check });
    console.log(check ? `checked ${id}: ${out} not written` : `wrote ${out} (validation.pass=${prov.validation.pass})`);
  } catch (e) {
    console.error(`kit-provenance: ${e.message}`);
    process.exit(1);
  }
}
