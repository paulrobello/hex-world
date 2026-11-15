// The preference storage wrappers (storage.ts): a value round trips, and a storage that throws on
// every access (blocked site data, a full quota) is never an exception for the caller.
import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { readStored, removeStored, writeStored } from "../src/storage.ts";

const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
afterEach(() => {
  if (original) Object.defineProperty(globalThis, "localStorage", original);
  else Reflect.deleteProperty(globalThis, "localStorage");
});

const install = (store: object) =>
  Object.defineProperty(globalThis, "localStorage", { value: store, configurable: true, writable: true });

const memory = () => {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  };
};

const refusing = () => {
  const boom = () => {
    throw new DOMException("blocked", "SecurityError");
  };
  return { getItem: boom, setItem: boom, removeItem: boom };
};

test("a stored value round trips, and removing it reads as none", () => {
  install(memory());
  assert.equal(readStored("k"), null);
  assert.equal(writeStored("k", "v"), true);
  assert.equal(readStored("k"), "v");
  removeStored("k");
  assert.equal(readStored("k"), null);
});

test("a storage that refuses every access is a null, a false and a no-op, never a throw", () => {
  install(refusing());
  assert.equal(readStored("k"), null);
  assert.equal(writeStored("k", "v"), false);
  assert.doesNotThrow(() => removeStored("k"));
});

test("a page whose localStorage getter throws reads as no storage", () => {
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    get() {
      throw new DOMException("blocked", "SecurityError");
    },
  });
  assert.equal(readStored("k"), null);
  assert.equal(writeStored("k", "v"), false);
  assert.doesNotThrow(() => removeStored("k"));
});
