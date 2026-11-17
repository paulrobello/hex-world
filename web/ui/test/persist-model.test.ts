// The start-up resume decision and the crash flag, against a Map-backed sessionStorage stub.
import assert from "node:assert/strict";
import { test } from "node:test";
import { clearResumeFlag, raiseResumeFlag, resumeFlagRaised, startupResume } from "../src/persist-model.ts";

// Node has no sessionStorage; the helpers fall through to their no-storage paths without one.
const backing = new Map<string, string>();
globalThis.sessionStorage = {
  getItem: (k: string) => backing.get(k) ?? null,
  setItem: (k: string, v: string) => {
    backing.set(k, v);
  },
  removeItem: (k: string) => {
    backing.delete(k);
  },
} as Storage;

test("startupResume: ?safe=1 opens safe; a bare ?safe does too; other addresses resume", () => {
  assert.equal(startupResume("?safe=1", false), "safe");
  assert.equal(startupResume("?safe", false), "safe");
  assert.equal(startupResume("?grade=cinematic&safe=1", false), "safe");
  assert.equal(startupResume("", false), "resume");
  assert.equal(startupResume("?grade=cozy", false), "resume");
  assert.equal(startupResume("?safe=0", false), "resume");
});

test("the crash flag flips the next start to safe until an answer clears it", () => {
  assert.equal(resumeFlagRaised(), false, "a fresh session has no flag");
  raiseResumeFlag("w-1");
  assert.equal(resumeFlagRaised(), true);
  assert.equal(startupResume("", resumeFlagRaised()), "safe");
  clearResumeFlag();
  assert.equal(resumeFlagRaised(), false);
  assert.equal(startupResume("", false), "resume");
});
