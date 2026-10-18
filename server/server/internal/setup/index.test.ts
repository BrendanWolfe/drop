import assert from "node:assert/strict";
import test from "node:test";
import {
  completeSetup,
  enabledSetupAdminWhere,
  ensureSetupToken,
  hasEnabledSetupAdmin,
} from "./index.ts";

function createClient(count: number) {
  const countCalls: Array<{ where: unknown }> = [];
  const deleteCalls: Array<{ where: unknown }> = [];
  const createCalls: Array<{ data: unknown }> = [];

  return {
    countCalls,
    deleteCalls,
    createCalls,
    client: {
      user: {
        async count(args: { where: unknown }) {
          countCalls.push(args);
          return count;
        },
      },
      aPIToken: {
        async deleteMany(args: { where: unknown }) {
          deleteCalls.push(args);
        },
        async create(args: { data: unknown }) {
          createCalls.push(args);
          return { token: "boot-token" };
        },
      },
    },
  };
}

test("only enabled admins with an enabled login mechanism count", async () => {
  const { client, countCalls } = createClient(0);

  assert.equal(await hasEnabledSetupAdmin(client), false);
  assert.deepEqual(countCalls[0]?.where, enabledSetupAdminWhere);
  assert.equal(enabledSetupAdminWhere.admin, true);
  assert.equal(enabledSetupAdminWhere.enabled, true);
  assert.deepEqual(enabledSetupAdminWhere.id, { not: "system" });
  assert.deepEqual(enabledSetupAdminWhere.authMecs, {
    some: { enabled: true },
  });
});

test("setup cannot complete or delete the token without an admin", async () => {
  const { client, deleteCalls } = createClient(0);

  assert.equal(await completeSetup(client), false);
  assert.equal(deleteCalls.length, 0);
});

test("setup completes and deletes setup tokens once an admin exists", async () => {
  const { client, deleteCalls } = createClient(1);

  assert.equal(await completeSetup(client), true);
  assert.deepEqual(deleteCalls, [
    {
      where: {
        mode: "System",
        acls: {
          hasSome: ["setup"],
        },
      },
    },
  ]);
});

test("startup preserves an existing setup token when users already exist", async () => {
  const { client, countCalls, deleteCalls, createCalls } = createClient(1);

  assert.equal(await ensureSetupToken(client), undefined);
  assert.deepEqual(countCalls[0]?.where, { id: { not: "system" } });
  assert.equal(deleteCalls.length, 0);
  assert.equal(createCalls.length, 0);
});

test("fresh startup rotates and creates a setup token when no users exist", async () => {
  const { client, deleteCalls, createCalls } = createClient(0);

  const token = await ensureSetupToken(client);
  assert.deepEqual(token, { token: "boot-token" });
  assert.deepEqual(deleteCalls, [
    {
      where: {
        mode: "System",
        acls: {
          hasSome: ["setup"],
        },
      },
    },
  ]);
  assert.deepEqual(createCalls, [
    {
      data: {
        name: "Setup Wizard",
        mode: "System",
        acls: ["setup"],
      },
    },
  ]);
});
