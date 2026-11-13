import assert from "node:assert/strict";
import test from "node:test";
import { recordDownloadActivity } from "./history.ts";
import type { DownloadActivityBody } from "./validation.ts";

const identity = {
  clientId: "desktop",
  userId: "player",
  userName: "Player",
  deviceName: "Desktop",
  gameTitle: "Game",
};
const body: typeof DownloadActivityBody.infer = {
  sessionId: "attempt",
  gameId: "game",
  versionId: "version",
  platform: "Linux",
  state: "Queued",
  occurredAt: 1000,
  bytesDownloaded: 0,
  bytesTotal: 5_000_000_000,
};

// A small conditional-write store exercises the same predicates used by SQL,
// without requiring a live database in the unit-test runner.
function store() {
  const rows = new Map<string, Record<string, unknown>>();
  const writer: Parameters<typeof recordDownloadActivity>[0] = {
    async upsert(args) {
      const key = args.where.clientId_sessionId!;
      const id = `${key.clientId}/${key.sessionId}`;
      if (!rows.has(id)) rows.set(id, { ...args.create });
    },
    async updateMany(args) {
      const where = args.where!;
      const row = rows.get(`${where.clientId}/${where.sessionId}`);
      if (
        !row ||
        row.endedAt !== null ||
        ("startedAt" in where && row.startedAt !== null)
      )
        return;
      const before = where.updatedAt as { lte?: Date } | undefined;
      if (
        before?.lte &&
        (row.updatedAt as Date).getTime() > before.lte.getTime()
      )
        return;
      Object.assign(row, args.data);
    },
  };
  return { rows, writer };
}

test("download attempts save who, what, start/end times and large byte counts", async () => {
  const { rows, writer } = store();
  await recordDownloadActivity(writer, identity, body);
  await recordDownloadActivity(writer, identity, {
    ...body,
    state: "Downloading",
    occurredAt: 2000,
  });
  await recordDownloadActivity(writer, identity, {
    ...body,
    state: "Completed",
    occurredAt: 3000,
    bytesDownloaded: body.bytesTotal,
  });
  const row = [...rows.values()][0]!;
  assert.equal(row.userName, "Player");
  assert.equal(row.gameTitle, "Game");
  assert.equal((row.queuedAt as Date).getTime(), 1000);
  assert.equal((row.startedAt as Date).getTime(), 2000);
  assert.equal((row.endedAt as Date).getTime(), 3000);
  assert.equal(row.bytesDownloaded, 5_000_000_000n);
});
test("retries and out-of-order updates never duplicate or reopen terminal records", async () => {
  for (const state of ["Completed", "Cancelled", "Failed"] as const) {
    const { rows, writer } = store();
    const ended = { ...body, state, occurredAt: 3000 };
    await recordDownloadActivity(writer, identity, body);
    await recordDownloadActivity(writer, identity, ended);
    await recordDownloadActivity(writer, identity, ended);
    await recordDownloadActivity(writer, identity, {
      ...body,
      state: "Downloading",
      occurredAt: 2000,
    });
    await recordDownloadActivity(writer, identity, {
      ...body,
      state: "Downloading",
      occurredAt: 4000,
    });
    assert.equal(rows.size, 1);
    assert.equal([...rows.values()][0]!.state, state);
  }
});
test("resuming preserves the first start time and different devices/attempts stay distinct", async () => {
  const { rows, writer } = store();
  await recordDownloadActivity(writer, identity, {
    ...body,
    state: "Downloading",
    occurredAt: 2000,
  });
  await recordDownloadActivity(writer, identity, {
    ...body,
    state: "Downloading",
    occurredAt: 4000,
  });
  await recordDownloadActivity(
    writer,
    { ...identity, clientId: "laptop" },
    body,
  );
  await recordDownloadActivity(writer, identity, {
    ...body,
    sessionId: "second-attempt",
  });
  assert.equal(rows.size, 3);
  assert.equal(
    (rows.get("desktop/attempt")!.startedAt as Date).getTime(),
    2000,
  );
});

test("a delayed queued event cannot undo a start, and clock changes cannot hide completion", async () => {
  const { rows, writer } = store();
  await recordDownloadActivity(writer, identity, {
    ...body,
    state: "Downloading",
    occurredAt: 2000,
  });
  await recordDownloadActivity(writer, identity, { ...body, occurredAt: 2000 });
  assert.equal(rows.get("desktop/attempt")!.state, "Downloading");
  await recordDownloadActivity(writer, identity, {
    ...body,
    state: "Completed",
    occurredAt: 1500,
  });
  assert.equal(rows.get("desktop/attempt")!.state, "Completed");
});
