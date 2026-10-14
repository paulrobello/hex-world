// Usage: node scripts/refs/fetch-result.mjs <job.json> <out.jpg>
//        node scripts/refs/fetch-result.mjs --job-id <job.json>
// Writes <out.jpg> (JPEG q90) and prints the sha256 of the lossless PNG encode of the
// Higgsfield output on stdout, recorded as refs.json `sourceSha256`.
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
// ESM ignores NODE_PATH; the pinned sharp lives only in scripts/assets/node_modules.
const sharp = createRequire(new URL("../assets/package.json", import.meta.url))("sharp");

const args = process.argv.slice(2);
const idOnly = args[0] === "--job-id";
const [jobPath, out] = idOnly ? args.slice(1) : args;
const job = JSON.parse(await readFile(jobPath, "utf8"));
const first = Array.isArray(job) ? job[0] : job;

if (idOnly) {
  console.log(first?.id ?? first?.job_id ?? first?.jobId ?? "");
  process.exit(0);
}

// result_url is the full image; min_result_url is a low-res webp preview and must never be used.
const url = first?.result_url;
if (!url) { console.error("no result_url in", jobPath); process.exit(1); }
const res = await fetch(url, { signal: AbortSignal.timeout(120_000) });
if (!res.ok) { console.error("download failed", res.status, url); process.exit(1); }
const png = await sharp(Buffer.from(await res.arrayBuffer())).png().toBuffer();
await sharp(png).jpeg({ quality: 90 }).toFile(out);
console.error("wrote", out);
console.log(createHash("sha256").update(png).digest("hex"));
