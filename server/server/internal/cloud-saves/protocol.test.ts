import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { Readable, Writable } from "node:stream";
import { pipeline } from "node:stream/promises";
import {
  advanceRevision,
  snapshotIdsToPrune,
  snapshotMeter,
} from "./protocol.ts";

test("two devices cannot advance the same cloud revision", async () => {
  let revision = 0;
  const writer = {
    async updateMany(args: {
      where: { id: string; revision: number };
      data: { revision: { increment: number } };
    }) {
      if (args.where.id !== "head" || args.where.revision !== revision)
        return { count: 0 };
      revision += args.data.revision.increment;
      return { count: 1 };
    },
  };
  const results = await Promise.allSettled([
    advanceRevision(writer, "head", 0),
    advanceRevision(writer, "head", 0),
  ]);
  assert.equal(results.filter((r) => r.status === "fulfilled").length, 1);
  assert.equal(revision, 1);
  await assert.rejects(advanceRevision(writer, "head", 0), { statusCode: 409 });
  assert.equal(await advanceRevision(writer, "head", 1), 2);
});

test("background retention preserves the latest confirmed snapshot", () => {
  const snapshots = Array.from({ length: 8 }, (_, revision) => ({
    id: String(revision),
    revision,
    kind: revision === 0 || revision === 2 ? "manual" : "background",
  }));
  assert.deepEqual(snapshotIdsToPrune(snapshots, 3).sort(), [
    "0",
    "1",
    "3",
    "4",
  ]);
  assert.equal(snapshots.length - snapshotIdsToPrune(snapshots, 3).length, 4);
  assert.deepEqual(
    snapshotIdsToPrune([{ id: "only", revision: 1, kind: "background" }], 0),
    [],
  );
});

test("archive size is checked during streaming and checksums before publication", async () => {
  const payload = Buffer.from("save archive");
  const checksum = createHash("sha256").update(payload).digest("hex");
  const sink = () =>
    new Writable({
      write(_chunk, _encoding, done) {
        done();
      },
    });
  const valid = snapshotMeter(payload.length);
  await pipeline(Readable.from([payload]), valid.stream, sink());
  assert.equal(valid.verify(checksum), payload.length);
  const oversized = snapshotMeter(payload.length - 1);
  await assert.rejects(
    pipeline(Readable.from([payload]), oversized.stream, sink()),
    { statusCode: 413 },
  );
  const corrupt = snapshotMeter(payload.length);
  await pipeline(Readable.from([payload]), corrupt.stream, sink());
  assert.throws(() => corrupt.verify("0".repeat(64)), { statusCode: 400 });
  assert.throws(() => snapshotMeter(10).verify(checksum), { statusCode: 400 });
});
