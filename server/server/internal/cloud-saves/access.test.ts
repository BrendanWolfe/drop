import assert from "node:assert/strict";
import { mock, test } from "node:test";

const moduleMock = (
  mock as unknown as {
    module: (
      specifier: string,
      options: {
        defaultExport?: unknown;
        namedExports?: Record<string, unknown>;
      },
    ) => void;
  }
).module.bind(mock);

let game: {
  id: string;
  cloudSavesEnabled: boolean;
  cloudSaveRevision: number;
  cloudSaveConfig: { locations: unknown[] } | null;
} | null;
const ageFilter = { NOT: { ageRatings: { some: { rating: "restricted" } } } };
const findFirst = mock.fn(async (_args: unknown) => game);
const getAgeRestrictionFilter = mock.fn(
  async (_id: string, _admin: boolean) => ageFilter,
);
moduleMock(new URL("../db/database.ts", import.meta.url).href, {
  defaultExport: { game: { findFirst } },
});
moduleMock(new URL("../utils/ageRestrictions.ts", import.meta.url).href, {
  namedExports: { getAgeRestrictionFilter },
});
const { requireCloudSaveAccess, requireCloudSaveCapability } =
  await import("./access.ts");
const user = { id: "authenticated-user", enabled: true, admin: false };
const client = { capabilities: ["CloudSaves"] };

test("cloud save access requires the existing device capability", () => {
  assert.throws(() => requireCloudSaveCapability({ capabilities: [] }), {
    statusCode: 403,
  });
  assert.doesNotThrow(() => requireCloudSaveCapability(client));
});

test("disabled accounts and devices without permission cannot query save configurations", async () => {
  findFirst.mock.resetCalls();
  await assert.rejects(
    requireCloudSaveAccess("game", { ...user, enabled: false }, client),
    { statusCode: 403 },
  );
  await assert.rejects(
    requireCloudSaveAccess("game", user, { capabilities: [] }),
    { statusCode: 403 },
  );
  assert.equal(findFirst.mock.callCount(), 0);
});

test("game access is scoped to the authenticated owner's library and age restrictions", async () => {
  game = {
    id: "game",
    cloudSavesEnabled: true,
    cloudSaveRevision: 1,
    cloudSaveConfig: null,
  };
  findFirst.mock.resetCalls();
  const result = await requireCloudSaveAccess("game", user, client);
  assert.deepEqual(
    (findFirst.mock.calls[0]!.arguments[0] as { where: unknown }).where,
    {
      id: "game",
      type: "Game",
      collections: { some: { collection: { userId: user.id } } },
      AND: [ageFilter],
    },
  );
  assert.deepEqual(getAgeRestrictionFilter.mock.calls.at(-1)?.arguments, [
    user.id,
    false,
  ]);
  assert.deepEqual(result.cloudSaveLocations, []);
});

test("unknown games and games whose administrator opted out cannot be synced", async () => {
  game = null;
  await assert.rejects(requireCloudSaveAccess("game", user, client), {
    statusCode: 404,
  });
  game = {
    id: "game",
    cloudSavesEnabled: false,
    cloudSaveRevision: 1,
    cloudSaveConfig: null,
  };
  await assert.rejects(requireCloudSaveAccess("game", user, client), {
    statusCode: 403,
  });
});
