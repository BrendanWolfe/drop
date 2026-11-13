import assert from "node:assert/strict";
import test from "node:test";
import { ArkErrors, type } from "arktype";
import {
  consoleNameKey,
  consoleNameSchema,
  consoleTypeIdSchema,
} from "./console-types.ts";

test("console names trim and collapse spaces while retaining readable casing", () => {
  assert.equal(
    consoleNameSchema("  Nintendo   GameCube  "),
    "Nintendo GameCube",
  );
  assert.equal(consoleNameSchema("PlayStation 2"), "PlayStation 2");
  assert.equal(consoleNameSchema("ＰＳ２"), "PS2");
});

test("case, spacing and compatibility variants reuse one console key", () => {
  for (const name of ["GameCube", " gamecube ", "ＧａｍｅＣｕｂｅ"])
    assert.equal(consoleNameKey(name), "gamecube");
  assert.equal(consoleNameKey("  Nintendo   GameCube "), "nintendo gamecube");
});

test("console names reject empty, control characters, non-text and excessive length", () => {
  assert.equal(consoleNameSchema("a".repeat(80)), "a".repeat(80));
  for (const value of [
    "",
    "   ",
    "a".repeat(81),
    "a".repeat(201),
    "PS\n2",
    "PS\u00002",
    null,
    42,
    ["PS2"],
  ]) {
    const result: unknown = consoleNameSchema(value);
    assert.ok(result instanceof ArkErrors);
  }
});

test("console assignment accepts a UUID or null and old imports default to uncategorized", () => {
  const schema = type({ consoleTypeId: consoleTypeIdSchema.default(null) });
  const id = "64ed6877-b773-461e-a2fe-e20e055ed34d";
  assert.deepEqual(schema({}), { consoleTypeId: null });
  assert.deepEqual(schema({ consoleTypeId: null }), { consoleTypeId: null });
  assert.deepEqual(schema({ consoleTypeId: id }), { consoleTypeId: id });
  for (const consoleTypeId of ["", "PS2", 42, [], { id }])
    assert.ok(schema({ consoleTypeId }) instanceof ArkErrors);
});
