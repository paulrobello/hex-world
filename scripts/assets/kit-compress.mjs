// Stage -> runtime: meshopt GLB (FLOAT attributes, D7) + per-channel UASTC -> BC7/ASTC KTX2 (zstd) + PNG.
import { execFile, execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { copyFile, mkdir, readdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { availableParallelism } from "node:os";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { NodeIO } from "@gltf-transform/core";
import { EXTMeshoptCompression } from "@gltf-transform/extensions";
import { reorder } from "@gltf-transform/functions";
import { MeshoptEncoder } from "meshoptimizer";
import validator from "gltf-validator";
import { CHANNELS, GLB_MAX, KTX_BUDGETS, VK, fullLevels, kitItemDir, ktx2Header, ktxPlatformError, sha256 } from "./kit-lib.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const KTX = join(ROOT, "tools/ktx-4.4.2");
const ktxEnv = {
  ...process.env,
  PATH: `${KTX}/KTX-Software-4.4.2-Darwin-arm64-tools.pkg/Payload/usr/local/bin:${process.env.PATH}`,
  DYLD_LIBRARY_PATH: `${KTX}/KTX-Software-4.4.2-Darwin-arm64-library.pkg/Payload/usr/local/lib`,
};
export const UASTC_THREADS = 1;
const ktx = (...args) => execFileSync("ktx", args, { env: ktxEnv, stdio: ["ignore", "pipe", "pipe"] }).toString();
const ktxAsync = (...args) => new Promise((ok, fail) =>
  execFile("ktx", args, { env: ktxEnv }, (e, out, err) => (e ? fail(new Error(`ktx ${args[0]} ${args.at(-1)}: ${err || e.message}`)) : ok(out))));

// Runs at most n tasks at once. Every map encode and transcode is its own single-threaded ktx process,
// so wall time scales with cores while each process keeps the deterministic --threads 1 encode.
function limiter(n) {
  let active = 0;
  const queue = [];
  const next = () => {
    if (active >= n || queue.length === 0) return;
    active++;
    const { task, ok, fail } = queue.shift();
    task().then(ok, fail).finally(() => { active--; next(); });
  };
  return (task) => new Promise((ok, fail) => { queue.push({ task, ok, fail }); next(); });
}
const serial = (task) => task();

// Awaits every promise before throwing the first failure, so no ktx child is still writing into a
// work dir that the caller removes on error.
async function settleAll(promises) {
  const rs = await Promise.allSettled(promises);
  const bad = rs.find((r) => r.status === "rejected");
  if (bad) throw bad.reason;
  return rs.map((r) => r.value);
}

export async function compressGlb(src, dst) {
  const bytes = await readFile(src);
  const report = await validator.validateBytes(new Uint8Array(bytes));
  if (report.issues.numErrors > 0)
    throw new Error(`${src}: glTF validation errors: ${JSON.stringify(report.issues.messages.filter((m) => m.severity === 0))}`);
  await MeshoptEncoder.ready;
  const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.encoder": MeshoptEncoder });
  const doc = await io.readBinary(new Uint8Array(bytes));
  if (doc.getRoot().listTextures().length > 0) throw new Error(`${src} embeds images (R6: textures ship as sidecar files)`);
  await doc.transform(reorder({ encoder: MeshoptEncoder }));
  // QUANTIZE is the P0 setting; it leaves attributes FLOAT (asserted below), no KHR_mesh_quantization.
  doc.createExtension(EXTMeshoptCompression).setRequired(true)
    .setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE });
  const out = Buffer.from(await io.writeBinary(doc));
  const j = JSON.parse(out.subarray(20, 20 + out.readUInt32LE(12)).toString());
  if ((j.extensionsUsed ?? []).some((e) => e !== "EXT_meshopt_compression"))
    throw new Error(`${src}: unexpected extensions ${j.extensionsUsed}`);
  for (const a of j.accessors.filter((a) => a.type !== "SCALAR"))
    if (a.componentType !== 5126) throw new Error(`${src}: non-float vertex accessor (D7)`);
  if (out.length > GLB_MAX) throw new Error(`${src}: GLB ${out.length} B > ${GLB_MAX} B budget`);
  await mkdir(dirname(dst), { recursive: true });
  await writeFile(dst, out);
  return { bytes: out.length, sha256: sha256(out) };
}

// The UASTC source both families transcode from. --assign-tf is required: PNG carries no transfer
// function, and ktx otherwise assumes sRGB and lossily converts UNORM channels.
// --threads 1 makes the encode byte-deterministic: basisu splits the UASTC RDO pass into
// min(4, threads) jobs, and with more than one job the same PNG can encode to different bytes
// (seen under host load). --uastc-rdo-m cannot replace it: libktx 4.4.2 wires that flag to
// "favor simpler modes" instead of the RDO multithreading switch.
export async function encodeUastc(png, channel, path, run = serial) {
  const c = CHANNELS[channel];
  if (!c) throw new Error(`unknown channel ${channel}`);
  await mkdir(dirname(path), { recursive: true });
  await run(() => ktxAsync("create", "--format", c.srgb ? "R8G8B8A8_SRGB" : "R8G8B8A8_UNORM", "--assign-tf", c.srgb ? "srgb" : "linear",
    ...(c.normalize ? ["--normalize"] : []), "--encode", "uastc", "--uastc-quality", "4",
    "--uastc-rdo", "--uastc-rdo-l", String(c.rdo), "--threads", String(UASTC_THREADS), "--zstd", "18", "--generate-mipmap", png, path));
  return path;
}

// Decodes mip 0 of a UASTC KTX2 to an RGBA8 PNG.
export function decodeUastc(path, pngOut) {
  ktx("extract", "--transcode", "rgba8", path, pngOut);
  return pngOut;
}

export async function encodeMap(png, channel, outStem, run = serial) {
  const c = CHANNELS[channel];
  if (!c) throw new Error(`unknown channel ${channel}`);
  const tmp = await encodeUastc(png, channel, `${outStem}.uastc.ktx2`, run);
  const outs = await settleAll(["bc7", "astc"].map(async (family) => {
    const path = `${outStem}.${family}.ktx2`;
    await run(() => ktxAsync("transcode", "--target", family, "--zstd", "18", tmp, path));
    const buf = await readFile(path);
    const h = ktx2Header(buf);
    const want = VK[family][c.srgb ? "srgb" : "linear"];
    if (h.vkFormat !== want) throw new Error(`${path}: vkFormat ${h.vkFormat}, expected ${want} (${channel})`);
    if (h.levels !== fullLevels(h.width, h.height)) throw new Error(`${path}: ${h.levels} mip levels`);
    if (Math.max(h.width, h.height) > 2048) throw new Error(`${path}: ${h.width}x${h.height} > 2048`);
    return { channel, family, path, format: `${family.toUpperCase()}_${c.srgb ? "SRGB" : "UNORM"}`,
      transfer: c.srgb ? "srgb" : "linear", vkFormat: h.vkFormat, levels: h.levels, bytes: buf.length, sha256: sha256(buf) };
  }));
  await rm(tmp);
  const pngOut = `${outStem}.png`;
  await copyFile(png, pngOut);
  const pbuf = await readFile(pngOut);
  outs.push({ channel, family: "png", path: pngOut, format: "PNG", transfer: c.srgb ? "srgb" : "linear", bytes: pbuf.length, sha256: sha256(pbuf) });
  return outs;
}

// budgetClass is a KTX_BUDGETS key: "piece" (unique maps, default), "tileable" (a shared material set)
// or "hero". Output goes to a sibling temp dir that replaces outDir only after every budget passes,
// so a failed run leaves outDir and the report untouched. A piece stage (piece, the default) must
// hold the GLB and export report kit-build writes: without the GLB the swap would replace the
// shipped GLB with nothing. Material sets have neither. jobs bounds the concurrent ktx processes
// (default: the core count); the output bytes do not depend on it.
export async function compressDir({ stageDir, outDir, reportPath, name, budgetClass = "piece", budgets = KTX_BUDGETS, piece = true,
  jobs = availableParallelism() }) {
  const ktxMax = budgets[budgetClass];
  if (ktxMax === undefined) throw new Error(`${name}: unknown budget class "${budgetClass}" (${Object.keys(budgets)})`);
  const shown = stageDir.startsWith(ROOT + sep) ? relative(ROOT, stageDir) : stageDir;
  for (const f of piece ? [`${name}.glb`, "export-report.json"] : [])
    if (!existsSync(join(stageDir, f))) throw new Error(`${name}: stage ${shown} has no ${f}; re-run make kit-build PIECE=${name}`);
  const work = `${outDir}.tmp-${process.pid}`;
  await rm(work, { recursive: true, force: true });
  try {
    const report = await compressInto({ stageDir, work, outDir, name, budgetClass, ktxMax, jobs });
    await rm(outDir, { recursive: true, force: true });
    await rename(work, outDir);
    await mkdir(dirname(reportPath), { recursive: true });
    await writeFile(reportPath, JSON.stringify(report, null, 2) + "\n");
    return report;
  } catch (e) {
    await rm(work, { recursive: true, force: true });
    throw e;
  }
}

async function compressInto({ stageDir, work, outDir, name, budgetClass, ktxMax, jobs }) {
  const files = await readdir(stageDir);
  const finalPath = (p) => {
    const q = join(outDir, p.slice(work.length + 1));
    return q.startsWith(ROOT) ? q.slice(ROOT.length + 1) : q;
  };
  const report = { name, date: new Date().toISOString(), toolVersions: {
      gltfTransform: "4.3.0", meshoptimizer: "0.25.0", ktxSoftware: ktx("--version").trim(), node: process.version },
    settings: { meshopt: "EXT_meshopt_compression QUANTIZE (FLOAT attributes)", uastcQuality: 4, uastcThreads: UASTC_THREADS, zstd: 18, rdo: CHANNELS },
    textureEncodes: [], budgets: { class: budgetClass, ktxMax, ktxBytes: { bc7: 0, astc: 0 } } };
  await mkdir(work, { recursive: true });
  if (files.includes(`${name}.glb`)) {
    report.glb = await compressGlb(join(stageDir, `${name}.glb`), join(work, `${name}.glb`));
    report.budgets.glbBytes = report.glb.bytes;
  }
  const maps = files.sort().map((f) => [f, f.match(new RegExp(`^${name}\\.(albedo|normal|orm|emissive|masks)\\.png$`))?.[1]])
    .filter(([, ch]) => ch);
  // A map runs its UASTC encode, then its two transcodes side by side: 2 processes per map at most.
  const run = limiter(Math.max(1, Math.min(jobs, maps.length * 2)));
  const encoded = await settleAll(maps.map(([f, ch]) => encodeMap(join(stageDir, f), ch, join(work, `${name}.${ch}`), run)));
  for (const o of encoded.flat()) {
    report.textureEncodes.push({ ...o, path: finalPath(o.path) });
    if (o.family !== "png") report.budgets.ktxBytes[o.family] += o.bytes;
  }
  for (const [fam, b] of Object.entries(report.budgets.ktxBytes))
    if (b > ktxMax) throw new Error(`${name}: ${fam} KTX2 total ${b} B > ${ktxMax} B (${budgetClass} budget)`);
  return report;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const platformError = ktxPlatformError(process.platform, process.arch);
  if (platformError) {
    console.error(`kit-compress: ${platformError}`);
    process.exit(1);
  }
  const [kind, name, ...rest] = process.argv.slice(2);
  const opt = (k, d) => { const i = rest.indexOf(`--${k}`); return i >= 0 ? rest[i + 1] : d; };
  const sub = kind === "material" ? `materials/${name}` : name;
  if (!["piece", "material"].includes(kind) || !name) {
    console.error("usage: kit-compress.mjs <piece|material> <name> [--class piece|hero|tileable --stage d --out d --report f]");
    process.exit(2);
  }
  const stageDir = resolve(ROOT, opt("stage", `target/kit-stage/${sub}`));
  // Budget class: --class, else "budgetClass" in the stage's export-report.json, else tileable for a
  // material and piece otherwise.
  let budgetClass = opt("class", "");
  if (!budgetClass) {
    const exp = await readFile(join(stageDir, "export-report.json"), "utf8").then(JSON.parse, () => ({}));
    budgetClass = exp.budgetClass ?? (kind === "material" ? "tileable" : "piece");
  }
  let rep;
  try {
    // Default output: the item's folder under assets/kit as the runtime manifest lays it out.
    const out = opt("out") ?? join("assets/kit",
      kitItemDir(JSON.parse(await readFile(join(ROOT, "assets/kit/manifest.json"), "utf8")), kind, name));
    rep = await compressDir({ name, budgetClass, stageDir, piece: kind === "piece",
      outDir: resolve(ROOT, out),
      reportPath: resolve(ROOT, opt("report", `assets-source/kit/${sub}/compress-report.json`)) });
  } catch (e) {
    console.error(`kit-compress: ${e.message}`);
    process.exit(1);
  }
  console.log(`${name} [${rep.budgets.class}]: glb ${rep.glb?.bytes ?? "-"} B, ktx bc7 ${rep.budgets.ktxBytes.bc7} B, astc ${rep.budgets.ktxBytes.astc} B`);
}
