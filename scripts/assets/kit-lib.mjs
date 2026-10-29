// Shared tables for the kit compress step and checks. Mirrors crates/hex-assets/src/kit_manifest.rs.
import { createHash } from "node:crypto";

// R6: base colour and emissive are sRGB; normal, ORM and masks are linear.
export const CHANNELS = {
  albedo: { srgb: true, rdo: 0.5, normalize: false },
  emissive: { srgb: true, rdo: 0.5, normalize: false },
  normal: { srgb: false, rdo: 0.25, normalize: true },
  orm: { srgb: false, rdo: 0.5, normalize: false },
  masks: { srgb: false, rdo: 0.5, normalize: false },
};
export const VK = { bc7: { srgb: 146, linear: 145 }, astc: { srgb: 158, linear: 157 } };
export const GLB_MAX = 2_097_152; // PRD §10
// KTX2 bytes per family (BC7 and ASTC counted separately), by budget class. Owner ruling 2026-10-13.
// Mirrored by kit_manifest.rs KTX_BUDGETS; change both together (kit_gate::budgets_match_kit_lib).
export const KTX_BUDGETS = { piece: 1_500_000, tileable: 3_600_000, hero: 11_500_000 };
const ID = [0xab, 0x4b, 0x54, 0x58, 0x20, 0x32, 0x30, 0xbb, 0x0d, 0x0a, 0x1a, 0x0a];

export function ktx2Header(buf) {
  if (buf.length < 48 || !ID.every((b, i) => buf[i] === b)) throw new Error("not a KTX2 file");
  const u = (o) => buf.readUInt32LE(o);
  return { vkFormat: u(12), width: u(20), height: u(24), layers: u(32), faces: u(36), levels: u(40), supercompression: u(44) };
}
export const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");

// KTX-Software 4.4.2 is pinned to its macOS arm64 package (scripts/fetch-ktx.sh), so kit-compress
// and kit-material run only there. Returns the up-front error for any other host, null on it.
export function ktxPlatformError(platform, arch) {
  if (platform === "darwin" && arch === "arm64") return null;
  return `KTX-Software 4.4.2 is pinned to its macOS arm64 build (tools/ktx-4.4.2, scripts/fetch-ktx.sh), so kit-compress ` +
    `runs on macOS arm64 only and this host is ${platform} ${arch}. Run make kit-compress, kit-material and kit-fixture on an Apple Silicon Mac.`;
}

// The runtime folder (relative to the manifest's folder, "/"-separated) of a piece or tileable set:
// where its manifest entry puts its files (a piece's glb and maps, a set's maps), or, for an item
// not in the manifest yet (kit-compress runs before the entry is added), "<parent>/<id>" where
// <parent> is the one folder every listed sibling of that kind sits in. Throws when neither holds.
export function kitItemDir(manifest, kind, id) {
  const dirOf = (p) => p.slice(0, Math.max(0, p.lastIndexOf("/")));
  let entries;
  if (kind === "piece") entries = (manifest.pieces ?? []).map((p) => [p.id, [p.glb, ...Object.values(p.maps ?? {})]]);
  else if (kind === "material") entries = Object.entries(manifest.materials ?? {}).map(([k, m]) => [k, Object.values(m.maps ?? {})]);
  else throw new Error(`${id}: unknown kind ${kind} (piece or material)`);
  const own = entries.find(([k]) => k === id);
  const ownDirs = [...new Set((own?.[1] ?? []).filter(Boolean).map(dirOf))];
  if (ownDirs.length > 1) throw new Error(`${id}: manifest lists its files in more than one folder (${ownDirs.join(", ")})`);
  if (ownDirs.length === 1 && ownDirs[0]) return ownDirs[0];
  const parents = new Set();
  for (const [k, files] of entries)
    for (const d of files.filter(Boolean).map(dirOf))
      if (d.endsWith(`/${k}`)) parents.add(d.slice(0, -k.length - 1));
  if (parents.size === 1) return `${[...parents][0]}/${id}`;
  const why = parents.size ? `listed ${kind}s sit in ${[...parents].sort().join(", ")}` : `no ${kind} to follow`;
  throw new Error(`${id}: cannot derive its output folder from the manifest (${why}); pass --out`);
}
export const fullLevels = (w, h) => Math.floor(Math.log2(Math.max(w, h))) + 1;

// Why a re-encode's KTX2 bytes can differ from the committed ones: each encoder setting or ktx version
// that differs between the item's committed compress report and the new one. A report with no
// uastcThreads predates b6cd857 (2026-10-21: the RDO pass split across up to 4 threads); one with no
// uastcAslrOff predates 6e96b12 (2026-10-26: the encode ran with ASLR on). Both encoded the same PNG to
// other bytes than today's encoder does, so a recompress of those items changes their KTX2 files.
const LEGACY = {
  uastcThreads: "absent (encoded before b6cd857, 2026-10-21, with the RDO pass split across up to 4 threads)",
  uastcAslrOff: "absent (encoded before 6e96b12, 2026-10-26, with ASLR on)",
};
export function encodeSettingsDrift(committed, current) {
  const canon = (v) => JSON.stringify(v, (_, x) => (x && typeof x === "object" && !Array.isArray(x)
    ? Object.fromEntries(Object.entries(x).sort(([a], [b]) => a.localeCompare(b))) : x));
  const was = committed?.settings ?? {}, now = current?.settings ?? {};
  const out = [];
  for (const k of [...new Set([...Object.keys(was), ...Object.keys(now)])].sort()) {
    if (canon(was[k]) === canon(now[k])) continue;
    const from = k in was ? canon(was[k]) : (LEGACY[k] ?? "absent");
    out.push(`settings.${k}: ${from} -> ${k in now ? canon(now[k]) : "absent"}`);
  }
  const [kv, kn] = [committed?.toolVersions?.ktxSoftware, current?.toolVersions?.ktxSoftware];
  if (kv !== kn) out.push(`toolVersions.ktxSoftware: ${kv ?? "absent"} -> ${kn ?? "absent"}`);
  return out;
}
