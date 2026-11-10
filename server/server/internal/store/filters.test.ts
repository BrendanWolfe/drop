import assert from "node:assert/strict";
import test from "node:test";
import { ArkErrors, type } from "arktype";
import { emulationFilterSchema, getEmulationFilter } from "./filters.ts";

test("selecting Emulated filters on the same flag as the store badge", () => {
  assert.deepEqual(getEmulationFilter("true"), { isEmulated: true });
});

test("selecting Native excludes emulated games", () => {
  assert.deepEqual(getEmulationFilter("false"), { isEmulated: false });
});

test("selecting both game types or neither leaves all games visible", () => {
  assert.deepEqual(getEmulationFilter(undefined), {});
  assert.deepEqual(getEmulationFilter("true,false"), {});
  assert.deepEqual(getEmulationFilter("false,true"), {});
});

test("game type filters combine with existing store restrictions", () => {
  const existing = {
    type: "Game",
    OR: [{ versions: { some: { launches: { some: { platform: "Linux" } } } } }],
    tags: { some: { id: { in: ["adventure"] } } },
    mAgeRating: { lte: 12 },
  };
  assert.deepEqual(
    { ...existing, ...getEmulationFilter("true") },
    { ...existing, isEmulated: true },
  );
});

test("the store accepts only supported game type selections", () => {
  const querySchema = type({ emulated: emulationFilterSchema.optional() });
  for (const value of ["true", "false", "true,false", "false,true"]) {
    assert.deepEqual(querySchema({ emulated: value }), { emulated: value });
  }
  assert.deepEqual(querySchema({}), {});
  for (const value of ["", "native", "1", "true,invalid", ["true"], true]) {
    assert.ok(querySchema({ emulated: value }) instanceof ArkErrors);
  }
});
