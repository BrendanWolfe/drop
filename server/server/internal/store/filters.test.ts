import assert from "node:assert/strict";
import test from "node:test";
import { ArkErrors, type } from "arktype";
import {
  emulationFilterSchema,
  getEmulationFilter,
  getNameFilter,
  nameSearchSchema,
} from "./filters.ts";

test("name search matches partial titles without case sensitivity and trims whitespace", () => {
  assert.deepEqual(getNameFilter("  elder SCROLLS  "), {
    mName: { contains: "elder SCROLLS", mode: "insensitive" },
  });
});

test("empty name searches leave the store unfiltered", () => {
  for (const value of [undefined, "", "   "])
    assert.deepEqual(getNameFilter(value), {});
});

test("name search treats SQL wildcards and backslashes literally", () => {
  assert.deepEqual(getNameFilter("100%_\\game"), {
    mName: { contains: "100\\%\\_\\\\game", mode: "insensitive" },
  });
  assert.deepEqual(getNameFilter("Pokémon & Tom's + Game"), {
    mName: { contains: "Pokémon & Tom's + Game", mode: "insensitive" },
  });
});

test("name search preserves existing store restrictions", () => {
  const existing = {
    type: "Game",
    tags: { some: { id: { in: ["adventure"] } } },
    mAgeRating: { lte: 12 },
    ...getEmulationFilter("true"),
  };
  assert.deepEqual(
    { ...existing, ...getNameFilter("Skyrim") },
    {
      ...existing,
      mName: { contains: "Skyrim", mode: "insensitive" },
    },
  );
});

test("name search query accepts bounded strings only", () => {
  const schema = type({ q: nameSearchSchema.optional() });
  assert.deepEqual(schema({}), {});
  assert.deepEqual(schema({ q: "x".repeat(200) }), { q: "x".repeat(200) });
  for (const q of ["x".repeat(201), ["game"], 42, null])
    assert.ok(schema({ q }) instanceof ArkErrors);
});

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
