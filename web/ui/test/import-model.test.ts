// The import preview (S10.6), the refusal checks (S10.8) and the load confirm's words (S10.3).
import assert from "node:assert/strict";
import { test } from "node:test";
import type { InspectResult, PlayTile } from "../src/bridge.ts";
import { dayText, kindRows, previewOf, refusedOf, sniff, unsavedText } from "../src/import-model.ts";

type Refusal = Extract<InspectResult, { ok: false }>;
const refusal = (code: Refusal["code"], reason: string, version: number | null = null): Refusal => ({
  ok: false,
  code,
  reason,
  version,
  supported: 3,
});
/** A header of a real file: HEXWORLD, version `v` little-endian, then a payload length. */
const header = (v: number): Uint8Array => new Uint8Array([72, 69, 88, 87, 79, 82, 76, 68, v, 0, 9, 0, 0, 0, 1, 2]);
const states = (r: ReturnType<typeof refusedOf>) => r.checks.map((c) => c.state).join(" ");
const tile = (q: number, r: number, id: string, rotation = 0): PlayTile => ({ q, r, tile: id, rotation, level: 0 });

test("sniff names the files players pick by mistake, else shows their first bytes", () => {
  assert.equal(sniff(new Uint8Array([0x50, 0x4b, 3, 4, 0])), "PK, which is a .zip archive");
  assert.equal(sniff(new Uint8Array([0x89, 0x50, 0x4e, 0x47, 13, 10])), "PNG, which is a PNG image");
  assert.equal(sniff(new Uint8Array([0x25, 0x50, 0x44, 0x46, 0x2d])), "%PDF, which is a PDF document");
  assert.equal(sniff(new TextEncoder().encode('{"tiles":[]}')), '{"tiles", which is JSON text');
  assert.equal(sniff(new TextEncoder().encode("hexworld")), "hexworld");
  assert.equal(sniff(new Uint8Array([1, 2, 0xfe, 0x10, 9])), "01 02 FE 10");
});

test("a zip: the header fails with what it is, later checks are not reached, and the advice is to unzip", () => {
  const head = new Uint8Array([0x50, 0x4b, 3, 4, 20, 0, 0, 0, 8, 0, 1, 2, 3, 4, 5, 6]);
  const r = refusedOf("harbour-town.zip", head, 24_576, refusal("bad_magic", "not a .hexworld file"));
  assert.equal(r.title, "This isn't a Hex World file");
  assert.equal(r.file, "harbour-town.zip · 24 KB");
  assert.equal(states(r), "ok bad skip skip skip");
  assert.equal(r.checks[0]?.text, "Read the file, 24 KB");
  assert.equal(r.checks[1]?.text, "Header: expected HEXWORLD, found PK, which is a .zip archive");
  assert.equal(r.checks[2]?.text, "Format version, not checked");
  assert.equal(r.checks[4]?.text, "Tiles and sockets, not checked");
  assert.equal(r.todo, "Unzip it first, then import the .hexworld file inside.");
});

test("a newer file: header passes, the version fails with both versions, and the advice is to reload", () => {
  const r = refusedOf(
    "harbour-town.hexworld",
    header(4),
    31_744,
    refusal("newer_version", "format version 4 is not supported (this build reads 1..=3)", 4),
  );
  assert.equal(r.title, "Made by a newer Hex World");
  assert.equal(states(r), "ok ok bad skip skip");
  assert.equal(r.checks[1]?.text, "Header: HEXWORLD");
  assert.equal(r.checks[2]?.text, "Format v4. This version reads up to v3.");
  assert.match(r.todo, /^Reload the page to get the newest Hex World/);
});

test("a damaged file: header and version pass, the checksum fails", () => {
  const r = refusedOf(
    "riverside.hexworld",
    header(3),
    900,
    refusal("checksum", "checksum mismatch: stored 0x1, computed 0x2"),
  );
  assert.equal(r.title, "This file is damaged");
  assert.equal(states(r), "ok ok ok bad skip");
  assert.equal(r.checks[2]?.text, "Format v3", "the version comes from the header bytes");
  assert.match(r.checks[3]?.text ?? "", /^Checksum: doesn't match/);
});

test("cut short, extra bytes, an old version that fails later, a bad world, too large and a tiny file", () => {
  const cut = refusedOf("a.hexworld", header(3), 120, refusal("truncated", "file is truncated: 120 of 129 bytes"));
  assert.equal(states(cut), "ok ok ok bad skip");
  assert.equal(cut.checks[3]?.text, "Length: 120 of the 129 bytes its header names, so the file is cut short");
  const extra = refusedOf(
    "a.hexworld",
    header(3),
    130,
    refusal("trailing_bytes", "1 unexpected bytes after the checksum"),
  );
  assert.equal(extra.checks[3]?.text, "Length: 1 extra byte after the end of the world");
  const world = refusedOf(
    "a.hexworld",
    header(1),
    200,
    refusal("invalid_world", "invalid world: tile q=1 r=0 is unknown"),
  );
  assert.equal(states(world), "ok ok ok ok bad");
  assert.equal(world.checks[2]?.text, "Format v1, updated to v3 when it opens");
  assert.equal(world.checks[4]?.text, "Tiles and sockets: invalid world: tile q=1 r=0 is unknown");
  const big = refusedOf(
    "huge.hexworld",
    header(3),
    17 * 1024 * 1024,
    refusal("too_large", "x bytes is over the limit"),
  );
  assert.equal(states(big), "bad skip skip skip skip");
  assert.equal(big.checks[0]?.text, "Read the file: 17.0 MB, over the 16 MB a world file can be");
  const tiny = refusedOf(
    "t.hexworld",
    new Uint8Array([72, 69, 88]),
    3,
    refusal("truncated", "file is truncated: 3 of 18 bytes"),
  );
  assert.equal(states(tiny), "ok bad skip skip skip");
  assert.equal(tiny.checks[1]?.text, "Header: the file ends after 3 bytes");
  const empty = refusedOf(
    "e.hexworld",
    new Uint8Array([]),
    0,
    refusal("truncated", "file is truncated: 0 of 18 bytes"),
  );
  assert.equal(empty.checks[1]?.text, "Header: the file is empty");
  const v0 = refusedOf(
    "z.hexworld",
    header(0),
    40,
    refusal("unsupported_version", "format version 0 is not supported", 0),
  );
  assert.equal(v0.checks[2]?.text, "Format v0. Hex World reads v1 to v3.");
});

test("preview: the file line, and per kind the tiles that carry it", () => {
  const tiles = [tile(0, 0, "meadow"), tile(1, 0, "road_straight"), tile(-1, 0, "forest")];
  const pv = previewOf("harbour-town.hexworld", 31_744, "harbour-town", { ok: true, version: 3, supported: 3, tiles });
  assert.equal(pv.name, "harbour-town");
  assert.equal(pv.file, "harbour-town.hexworld · 31 KB · format v3");
  const rows = Object.fromEntries(pv.rows.map((r) => [r.kind, r.tiles]));
  assert.equal(rows.Grass, 2, "meadow, and the road's verges");
  assert.equal(rows.Road, 1);
  assert.equal(rows.Forest, 1);
  assert.deepEqual(kindRows([]), []);
  assert.deepEqual(
    pv.rows.map((r) => r.kind),
    ["Grass", "Forest", "Road"],
    "legend order",
  );
});

test("load confirm and preview words", () => {
  assert.equal(
    unsavedText("Riverside", 0, 5_000),
    "Riverside has changes from the last few seconds that aren't saved yet.",
  );
  assert.equal(
    unsavedText("Riverside", 0, 90_000),
    "Riverside has changes from the last minute that aren't saved yet.",
  );
  assert.equal(
    unsavedText("Riverside", 0, 150_000),
    "Riverside has changes from the last 2 minutes that aren't saved yet.",
  );
  assert.equal(dayText(new Date(2026, 9, 11, 18, 2).getTime()), "11 Oct 2026");
});
