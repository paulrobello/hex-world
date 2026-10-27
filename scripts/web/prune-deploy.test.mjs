// prune-deploy.mjs: removes the listed files and directories, keeps everything else, and refuses a
// list entry the build no longer has (so a renamed fixture or cue cannot slip back into the deploy).
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { NOT_DEPLOYED, pruneDeploy } from "./prune-deploy.mjs";

function dist() {
  const dir = mkdtempSync(join(tmpdir(), "prune-deploy-"));
  for (const p of ["assets/kit/p0/hex_test.glb", "assets/kit/p1/fixture/fixture.glb", "assets/kit/p0/keep.glb"]) {
    mkdirSync(join(dir, p, ".."), { recursive: true });
    writeFileSync(join(dir, p), "x".repeat(10));
  }
  return dir;
}

test("removes listed files and directories and keeps the rest", () => {
  const dir = dist();
  const bytes = pruneDeploy(dir, ["assets/kit/p0/hex_test.glb", "assets/kit/p1/fixture"]);
  assert.equal(bytes, 20);
  assert.ok(!existsSync(join(dir, "assets/kit/p0/hex_test.glb")));
  assert.ok(!existsSync(join(dir, "assets/kit/p1/fixture")));
  assert.ok(existsSync(join(dir, "assets/kit/p0/keep.glb")));
});

test("a listed path the build lacks fails before anything is removed", () => {
  const dir = dist();
  assert.throws(() => pruneDeploy(dir, ["assets/kit/p0/hex_test.glb", "assets/gone.glb"]), /not in .*assets\/gone\.glb/);
  assert.ok(existsSync(join(dir, "assets/kit/p0/hex_test.glb")));
});

test("every NOT_DEPLOYED entry exists in the source assets/ Trunk copies", () => {
  const repo = new URL("../../", import.meta.url).pathname;
  for (const p of NOT_DEPLOYED) assert.ok(existsSync(join(repo, p)), p);
});

test("the New world title ships: a played cue is never pruned", () => {
  assert.ok(!NOT_DEPLOYED.some((p) => p.includes("title-theme")));
});
