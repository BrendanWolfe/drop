import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import type { DownloadableMetadata } from "../types.ts";
import {
  addPendingDownload,
  pendingDownloadItems,
  pendingDropIndex,
  type PendingDownload,
} from "../utils/pending-downloads.ts";

const meta = (id: string, version = "v1"): DownloadableMetadata => ({
  id,
  version,
  targetPlatform: "Windows",
  downloadType: "Game" as DownloadableMetadata["downloadType"],
});
const candidate: PendingDownload = {
  meta: meta("partial"),
  name: "Partial game",
  action: "resume",
};

test("pending cards distinguish updates from partial installs without inventing progress", () => {
  const items = pendingDownloadItems(
    [
      candidate,
      { meta: meta("update"), name: "Update game", action: "update" },
    ],
    [],
  );
  assert.deepEqual(
    items.map((item) => item.status),
    ["Partially installed", "Update available"],
  );
  assert.equal(items[0]!.dl_progress, null);
  assert.equal(items[0]!.name, candidate.name);
  assert.equal(items[0]!.meta.targetPlatform, "Windows");
});

test("pending cards exclude queued games even when the queued version is newer", () => {
  assert.deepEqual(
    pendingDownloadItems([candidate], [{ meta: meta("partial", "v2") }]),
    [],
  );
});

test("pending drops map active and waiting targets, including an empty queue", () => {
  assert.equal(pendingDropIndex("active", 1, 3), 0);
  assert.equal(pendingDropIndex("waiting", 0, 3), 1);
  assert.equal(pendingDropIndex("waiting", 1, 3), 2);
  assert.equal(pendingDropIndex("waiting", 9, 3), 3);
  assert.equal(pendingDropIndex("waiting", 0, 0), 0);
  for (const [slot, index] of [
    ["pending", 0],
    ["waiting", -1],
    ["active", 0.5],
  ] as const) {
    assert.equal(pendingDropIndex(slot, index, 3), undefined);
  }
});

function controls(initial: string[], batch = [candidate.meta]) {
  const queue = initial.map((id) => ({ meta: meta(id) }));
  const moves: Array<{ id: string; index: number }> = [];
  const api = {
    readQueue: () => queue,
    enqueue: async (_meta: DownloadableMetadata) => {
      queue.push(...batch.map((meta) => ({ meta })));
      return batch;
    },
    wait: async (ready: () => boolean) => {
      assert.equal(ready(), true);
    },
    move: async (
      item: DownloadableMetadata,
      index: number,
      front: DownloadableMetadata,
    ) => {
      assert.deepEqual(front, queue[0]!.meta);
      const previous = queue.findIndex(({ meta }) => meta.id === item.id);
      const [entry] = queue.splice(previous, 1);
      queue.splice(index, 0, entry!);
      moves.push({ id: item.id, index });
    },
  };
  return { queue, moves, api };
}

test("Add to queue appends without interrupting or reordering existing downloads", async () => {
  const { queue, moves, api } = controls(["active", "next"]);
  await addPendingDownload(candidate, undefined, api);
  assert.deepEqual(
    queue.map(({ meta }) => meta.id),
    ["active", "next", "partial"],
  );
  assert.deepEqual(moves, []);
});

test("Download now promotes a pending game ahead of the current active download", async () => {
  const { queue, api } = controls(["active", "next"]);
  await addPendingDownload(candidate, 0, api);
  assert.deepEqual(
    queue.map(({ meta }) => meta.id),
    ["partial", "active", "next"],
  );
});

test("dropping into Up next respects the selected insertion position", async () => {
  const { queue, api } = controls(["active", "next", "last"]);
  await addPendingDownload(candidate, 2, api);
  assert.deepEqual(
    queue.map(({ meta }) => meta.id),
    ["active", "next", "partial", "last"],
  );
});

test("updates promote the new build, keeping required emulators before the game", async () => {
  const dependency = {
    ...meta("emulator"),
    downloadType: "Emulator" as DownloadableMetadata["downloadType"],
  };
  const updated = meta("partial", "v2");
  const { queue, moves, api } = controls(
    ["active", "next"],
    [dependency, updated],
  );
  await addPendingDownload({ ...candidate, action: "update" }, 0, api);
  assert.deepEqual(
    queue.map(({ meta }) => [meta.id, meta.version]),
    [
      ["emulator", "v1"],
      ["partial", "v2"],
      ["active", "v1"],
      ["next", "v1"],
    ],
  );
  assert.deepEqual(moves, [
    { id: "emulator", index: 0 },
    { id: "partial", index: 1 },
  ]);
});

test("pending downloads can start in an empty queue without a move", async () => {
  const { queue, moves, api } = controls([]);
  await addPendingDownload(candidate, 0, api);
  assert.equal(queue[0]!.meta.id, "partial");
  assert.deepEqual(moves, []);
});

test("dropping into Up next never moves an already active dependency", async () => {
  const dependency = meta("emulator");
  const { queue, moves, api } = controls(["emulator", "next"]);
  api.enqueue = async () => {
    queue.push({ meta: candidate.meta });
    return [dependency, candidate.meta];
  };
  await addPendingDownload(candidate, 1, api);
  assert.deepEqual(
    queue.map(({ meta }) => meta.id),
    ["emulator", "partial", "next"],
  );
  assert.deepEqual(moves, [{ id: "partial", index: 1 }]);
});

test("a changed active download is not interrupted after asynchronous preparation", async () => {
  const { queue, moves, api } = controls(["active", "next"]);
  const enqueue = api.enqueue;
  api.enqueue = async (meta) => {
    queue.shift();
    return enqueue(meta);
  };
  await assert.rejects(
    addPendingDownload(candidate, 0, api),
    /active download changed/,
  );
  assert.deepEqual(
    queue.map(({ meta }) => meta.id),
    ["next", "partial"],
  );
  assert.deepEqual(moves, []);
});

test("failed preparation leaves the queue unchanged", async () => {
  const { queue, api } = controls(["active"]);
  api.enqueue = async () => {
    throw new Error("Offline");
  };
  await assert.rejects(addPendingDownload(candidate, 0, api), /Offline/);
  assert.deepEqual(
    queue.map(({ meta }) => meta.id),
    ["active"],
  );
});

test("the pending section comes after Up next and only accepts outgoing drags", () => {
  const source = readFileSync(
    new URL("../pages/queue.vue", import.meta.url),
    "utf8",
  );
  assert.ok(
    source.indexOf('id="pending-heading"') >
      source.indexOf('id="queued-heading"'),
  );
  assert.match(source, /name: 'downloads', pull: true, put: false/);
  assert.match(source, /@enqueue="requestPendingDownload\(element\)"/);
  assert.match(source, /@activate="requestPendingDownload\(element, 0\)"/);
  assert.match(
    source,
    /pendingSwitch\.value = \{ meta: candidate\.meta, newIndex, front, candidate \}/,
  );
});

test("pending cards have two labeled icon actions with tooltips", () => {
  const source = readFileSync(
    new URL("../components/DownloadQueueCard.vue", import.meta.url),
    "utf8",
  );
  const pending = source.match(
    /<template v-if="pending">([\s\S]*?)<\/template>/,
  )?.[1];
  assert.ok(pending);
  assert.equal((pending.match(/<button/g) ?? []).length, 2);
  assert.match(pending, /title="Add to queue"/);
  assert.match(pending, /title="Download now"/);
  assert.equal((pending.match(/:aria-label=/g) ?? []).length, 2);
});
