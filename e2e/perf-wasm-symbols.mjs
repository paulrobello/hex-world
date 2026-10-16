// Map a stripped release profile only when every non-custom WASM section matches a named build.
// Usage: node e2e/perf-wasm-symbols.mjs <release.wasm> <named.wasm> <symbols.json>
import { readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
const [release, named, output] = process.argv.slice(2);
function sections(bytes) {
  let pos = 8;
  const uint = () => {
    let n = 0, shift = 0, b;
    do { b = bytes[pos++]; n += (b & 127) * 2 ** shift; shift += 7; } while (b & 128);
    return n;
  };
  const string = () => { const n = uint(); const text = bytes.subarray(pos, pos + n).toString("utf8"); pos += n; return text; };
  const core = [], names = {};
  while (pos < bytes.length) {
    const start = pos, id = bytes[pos++], size = uint(), end = pos + size;
    if (id !== 0) core.push(bytes.subarray(start, end));
    else if (string() === "name") {
      while (pos < end) {
        const type = bytes[pos++], length = uint(), next = pos + length;
        if (type === 1) {
          const count = uint();
          for (let i = 0; i < count; i++) { const index = uint(); names[index] = string(); }
        }
        pos = next;
      }
    }
    pos = end;
  }
  return { core_sha256: createHash("sha256").update(Buffer.concat(core)).digest("hex"), names };
}
const a = sections(await readFile(release));
const b = sections(await readFile(named));
const matched = a.core_sha256 === b.core_sha256;
await writeFile(output, JSON.stringify({ release, named, matched, release_core_sha256: a.core_sha256,
  named_core_sha256: b.core_sha256, names: matched ? b.names : {},
  note: matched ? "All non-custom sections identical. Function indexes safely match release profile." : "Bytecode differs. Do not map these symbols onto release profiles." }, null, 2) + "\n");
console.log(JSON.stringify({ matched, symbols: matched ? Object.keys(b.names).length : 0 }));
