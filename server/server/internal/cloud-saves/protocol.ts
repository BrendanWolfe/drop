import { createHash } from "node:crypto";
import { Transform } from "node:stream";
import { createError } from "h3";

type HeadWriter = {
  updateMany(args: {
    where: { id: string; revision: number };
    data: { revision: { increment: number } };
  }): Promise<{ count: number }>;
};

export async function advanceRevision(
  writer: HeadWriter,
  id: string,
  expected: number,
) {
  const updated = await writer.updateMany({
    where: { id, revision: expected },
    data: { revision: { increment: 1 } },
  });
  if (updated.count !== 1)
    throw createError({
      statusCode: 409,
      message:
        "Another device changed the cloud save. Choose local or cloud before continuing.",
    });
  return expected + 1;
}

export function snapshotIdsToPrune(
  snapshots: { id: string; revision: number; kind: string }[],
  history: number,
) {
  const sorted = snapshots.toSorted((a, b) => b.revision - a.revision);
  const keep = new Set(
    sorted.slice(0, Math.max(1, Math.min(20, history))).map((s) => s.id),
  );
  const confirmed = sorted.find((s) => s.kind === "manual");
  if (confirmed) keep.add(confirmed.id);
  return sorted.filter((s) => !keep.has(s.id)).map((s) => s.id);
}

export function snapshotMeter(limit: number) {
  let bytes = 0;
  const hash = createHash("sha256");
  return {
    stream: new Transform({
      transform(chunk: Buffer, _encoding, callback) {
        bytes += chunk.length;
        if (bytes > limit)
          return callback(
            createError({
              statusCode: 413,
              message: "Cloud save exceeds the server size limit.",
            }),
          );
        hash.update(chunk);
        callback(null, chunk);
      },
    }),
    verify(checksum: string) {
      if (!bytes || bytes > limit || hash.digest("hex") !== checksum)
        throw createError({
          statusCode: 400,
          message: "Snapshot checksum mismatch.",
        });
      return bytes;
    },
  };
}
