import assert from "node:assert/strict";
import test from "node:test";
import { ArkErrors, type } from "arktype";
import {
  emulationFilterSchema,
  consoleFilterSchema,
  combineStoreFilters,
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

const gameCube = "64ed6877-b773-461e-a2fe-e20e055ed34d";
const ps2 = "0150e859-081c-4613-a22c-f2e5eb8b2c80";

test("console selections match any selected console and only emulated games", () => {
  const expected = {
    isEmulated: true,
    consoleTypeId: { in: [gameCube, ps2] },
  };
  assert.deepEqual(getEmulationFilter("true", `${gameCube},${ps2}`), expected);
  assert.deepEqual(
    getEmulationFilter(undefined, `${gameCube},${ps2}`),
    expected,
  );
  assert.deepEqual(
    getEmulationFilter("true", `${gameCube},${ps2},${gameCube}`),
    expected,
  );
});

test("console subfilters preserve native games when both parent types are selected", () => {
  const expected = {
    OR: [
      { isEmulated: false },
      { isEmulated: true, consoleTypeId: { in: [gameCube] } },
    ],
  };
  assert.deepEqual(getEmulationFilter("true,false", gameCube), expected);
  assert.deepEqual(getEmulationFilter("false,true", gameCube), expected);
});

test("native-only filtering ignores stale console selections", () => {
  assert.deepEqual(getEmulationFilter("false", gameCube), {
    isEmulated: false,
  });
});

test("clearing console selections includes uncategorized emulated games again", () => {
  assert.deepEqual(getEmulationFilter("true", ""), { isEmulated: true });
  assert.deepEqual(getEmulationFilter("true,false", ""), {});
});

test("console queries accept at most 100 UUIDs, rejecting malformed and oversized input", () => {
  const schema = type({ consoles: consoleFilterSchema.optional() });
  for (const consoles of [
    gameCube,
    `${gameCube},${ps2}`,
    Array(100).fill(gameCube).join(","),
  ])
    assert.deepEqual(schema({ consoles }), { consoles });
  assert.deepEqual(schema({}), {});
  for (const consoles of [
    "",
    "GameCube",
    `${gameCube},`,
    `${gameCube},bad`,
    Array(101).fill(gameCube).join(","),
    [gameCube],
    null,
    true,
  ])
    assert.ok(schema({ consoles }) instanceof ArkErrors);
});

test("console OR clauses cannot overwrite platform, company, age, tag or name restrictions", () => {
  const platform = {
    OR: [
      {
        versions: {
          some: { launches: { some: { platform: "Linux" as const } } },
        },
      },
    ],
  };
  const company = { OR: [{ publishers: { some: { id: "publisher" } } }] };
  const age = {
    AND: [
      { ageRatings: { some: {} } },
      { NOT: { ageRatings: { some: { rating: "18" } } } },
    ],
  };
  const tags = { tags: { some: { id: "tag" } } };
  const consoles = getEmulationFilter("true,false", gameCube);
  const name = getNameFilter("Zelda");
  assert.deepEqual(
    combineStoreFilters(
      platform,
      company,
      undefined,
      age,
      tags,
      consoles,
      name,
    ),
    {
      type: "Game",
      AND: [platform, company, age, tags, consoles, name],
    },
  );
});
