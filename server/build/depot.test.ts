import assert from "node:assert/strict";
import { mock, test } from "node:test";

type Depot = { id: string; endpoint: string };
type Upsert = {
  where: { id: string };
  update: { endpoint: string };
  create: Depot;
};

const depots = new Map<string, Depot>();
const upsert = mock.fn(async (args: Upsert) => {
  const existing = depots.get(args.where.id);
  const depot = existing ? { ...existing, ...args.update } : args.create;
  depots.set(depot.id, depot);
  return depot;
});

const moduleMock = (
  mock as unknown as {
    module: (specifier: string, options: { defaultExport: unknown }) => void;
  }
).module.bind(mock);

moduleMock(new URL("../server/internal/db/database.ts", import.meta.url).href, {
  defaultExport: { depot: { upsert } },
});
Object.defineProperty(globalThis, "defineNitroPlugin", {
  value: (plugin: () => Promise<void>) => plugin,
  configurable: true,
});

const { default: plugin } =
  await import("../server/plugins/07.torrential-depot.ts");
const initializeDepot = plugin as unknown as () => Promise<void>;

test("startup advertises a relative built-in depot for new installations", async () => {
  depots.clear();
  await initializeDepot();
  assert.deepEqual(depots.get("torrential"), {
    id: "torrential",
    endpoint: "/api/v1/depot/",
  });
});

test("startup replaces the old absolute built-in URL without changing external depots", async () => {
  const external = {
    id: "external",
    endpoint: "https://cdn.example.com/games/",
  };
  depots.clear();
  depots.set("torrential", {
    id: "torrential",
    endpoint: "http://192.168.1.50:3000/api/v1/depot/",
  });
  depots.set(external.id, external);
  await initializeDepot();
  assert.equal(depots.get("torrential")?.endpoint, "/api/v1/depot/");
  assert.deepEqual(depots.get(external.id), external);
});
