// The HUD's fetch URLs carry the content hash scripts/web/version-dist.mjs inlined into the page.
import assert from "node:assert/strict";
import { test } from "node:test";
import { versioned } from "../src/versioned.ts";

const HASHES = { "assets/audio/manifest.json": "0123456789", "assets/thumbs/meadow-r0.webp": "abcdef0123" };

test("a mapped path gets ?v=<hash>; a missing one or a page without a map stays unversioned", () => {
  assert.equal(versioned("assets/audio/manifest.json", HASHES), "assets/audio/manifest.json?v=0123456789");
  assert.equal(versioned("./assets/thumbs/meadow-r0.webp", HASHES), "./assets/thumbs/meadow-r0.webp?v=abcdef0123");
  const warn = console.warn;
  const warned: unknown[] = [];
  console.warn = (...a: unknown[]) => warned.push(a.join(" "));
  try {
    assert.equal(versioned("assets/audio/sfx/new.webm", HASHES), "assets/audio/sfx/new.webm");
    assert.equal(versioned("assets/audio/sfx/new.webm", HASHES), "assets/audio/sfx/new.webm");
    assert.equal(versioned("assets/audio/manifest.json", undefined), "assets/audio/manifest.json");
    // Prototype keys are not hashes.
    assert.equal(versioned("constructor", HASHES), "constructor");
  } finally {
    console.warn = warn;
  }
  assert.equal(warned.filter((w) => String(w).includes("new.webm")).length, 1, "warned once per path");
  assert.equal(warned.filter((w) => String(w).includes("no window.__HEX_HASHES")).length, 1);
});
