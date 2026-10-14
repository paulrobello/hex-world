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
export const fullLevels = (w, h) => Math.floor(Math.log2(Math.max(w, h))) + 1;
