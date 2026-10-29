// Re-encodes every kit map whose committed PNG is the full-resolution source with KTX-Software
// 5.0.0-rc2 (ASLR on, no no-aslr.py wrapper), the kit-compress flags otherwise, and overwrites the
// committed BC7/ASTC files in this worktree so `make thumbs` shows the look change. Writes
// results.json: per map, old and new bytes and hashes per family. Measurement only, never landed.
// usage: node docs/perf/ktx5-rc2/measure.mjs <ktx5 tools bin> <ktx5 lib> [jobs]
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, rm, writeFile } from "node:fs/promises";
import { globSync } from "node:fs";
import { CHANNELS } from "../../../scripts/assets/kit-lib.mjs";

const [bin, lib, jobs = "8"] = process.argv.slice(2);
const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, DYLD_LIBRARY_PATH: lib };
const ktx = (args) => new Promise((ok, fail) => execFile("ktx", args, { env }, (e, _o, err) => (e ? fail(new Error(err || e.message)) : ok())));
const sha = (b) => createHash("sha256").update(b).digest("hex");

const work = [];
for (const f of globSync("assets-source/kit/**/compress-report.json")) {
  const r = JSON.parse(await readFile(f, "utf8"));
  const byKey = new Map(r.textureEncodes.map((t) => [`${t.channel}.${t.family}`, t]));
  for (const t of r.textureEncodes)
    if (t.family === "png" && t.size === undefined)
      work.push({ item: r.name, channel: t.channel, png: t.path, bc7: byKey.get(`${t.channel}.bc7`), astc: byKey.get(`${t.channel}.astc`) });
}
const results = [];
let next = 0;
async function worker() {
  while (next < work.length) {
    const w = work[next++];
    const c = CHANNELS[w.channel];
    const tmp = `${w.png}.rc2.uastc.ktx2`;
    await ktx(["create", "--format", c.srgb ? "R8G8B8A8_SRGB" : "R8G8B8A8_UNORM", "--assign-tf", c.srgb ? "srgb" : "linear",
      ...(c.normalize ? ["--normalize"] : []), "--encode", "uastc", "--uastc-quality", "4", "--uastc-rdo", "--uastc-rdo-l", String(c.rdo),
      "--threads", "1", "--zstd", "18", "--generate-mipmap", w.png, tmp]);
    const row = { item: w.item, channel: w.channel };
    for (const fam of ["bc7", "astc"]) {
      const old = w[fam];
      await ktx(["transcode", "--target", fam, "--zstd", "18", tmp, old.path]);
      const nb = await readFile(old.path);
      row[fam] = { oldBytes: old.bytes, newBytes: nb.length, changed: sha(nb) !== old.sha256 };
    }
    await rm(tmp);
    results.push(row);
    if (results.length % 50 === 0) console.log(`${results.length}/${work.length}`);
  }
}
await Promise.all(Array.from({ length: Number(jobs) }, worker));
const sum = (fam, k) => results.reduce((a, r) => a + r[fam][k], 0);
const summary = { maps: work.length, ktx: "5.0.0-rc2", changed: { bc7: results.filter((r) => r.bc7.changed).length, astc: results.filter((r) => r.astc.changed).length },
  bytes: Object.fromEntries(["bc7", "astc"].map((f) => [f, { old: sum(f, "oldBytes"), new: sum(f, "newBytes") }])) };
results.sort((a, b) => (a.item + a.channel).localeCompare(b.item + b.channel));
await writeFile("docs/perf/ktx5-rc2/results.json", JSON.stringify({ summary, results }, null, 1) + "\n");
console.log(JSON.stringify(summary));
