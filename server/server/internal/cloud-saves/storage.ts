import { randomUUID } from "node:crypto";
import type { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { createError } from "h3";
import prisma from "~/server/internal/db/database";
import objectHandler from "~/server/internal/objects";
import { applicationSettings } from "~/server/internal/config/application-configuration";
import { MAX_ARCHIVE_BYTES, type parseUploadHeaders } from "./validation";
import { advanceRevision, snapshotIdsToPrune, snapshotMeter } from "./protocol";

export async function saveLimits() {
  return {
    archiveBytes: Math.max(
      1,
      Math.min(
        MAX_ARCHIVE_BYTES,
        Math.floor(
          (await applicationSettings.get("saveSlotSizeLimit")) * 1024 * 1024,
        ),
      ),
    ),
    history: Math.max(
      1,
      Math.min(20, await applicationSettings.get("saveSlotHistoryLimit")),
    ),
  };
}

export async function cleanSaveGarbage(limit = 20) {
  // Pending uploads are also registered here. Never collect an active upload.
  const garbage = await prisma.cloudSaveGarbage.findMany({
    where: { createdAt: { lt: new Date(Date.now() - 60 * 60 * 1000) } },
    take: Math.min(limit, 1000),
  });
  for (const item of garbage) {
    if (await objectHandler.deleteAsSystem(item.objectId))
      await prisma.cloudSaveGarbage.deleteMany({
        where: { objectId: item.objectId },
      });
  }
}

export async function protectedCloudSaveObjects() {
  // A single statement observes the pending→published handoff atomically.
  // Separate model queries could straddle a commit and miss both references.
  const references = await prisma.$queryRaw<Array<{ objectId: string }>>`
    SELECT "objectId" FROM "CloudSaveSnapshot"
    UNION SELECT "objectId" FROM "CloudSaveGarbage"
  `;
  return new Set(references.map((row) => row.objectId));
}

export async function uploadSnapshot(
  stream: Readable,
  owner: {
    gameId: string;
    userId: string;
    namespace: string;
    clientId: string;
  },
  upload: ReturnType<typeof parseUploadHeaders>,
) {
  const limits = await saveLimits();
  const objectId = randomUUID();
  await prisma.cloudSaveGarbage.create({ data: { objectId } });
  try {
    // No generic object permissions: downloads must pass the save ownership check.
    const output = await objectHandler.createWithStream(
      objectId,
      { cloudSave: "1" },
      [],
    );
    if (!output) throw new Error("Unable to create cloud save object.");
    const meter = snapshotMeter(limits.archiveBytes);
    await pipeline(stream, meter.stream, output, {
      signal: AbortSignal.timeout(120_000),
    });
    const bytes = meter.verify(upload.checksum);

    return await prisma.$transaction(async (tx) => {
      // Hold the game's configuration stable until the snapshot is published.
      await tx.$queryRaw`SELECT "id" FROM "Game" WHERE "id" = ${owner.gameId} FOR SHARE`;
      const game = await tx.game.findUnique({ where: { id: owner.gameId } });
      if (
        !game?.cloudSavesEnabled ||
        game.cloudSaveRevision !== upload.definitionRevision
      )
        throw createError({
          statusCode: 409,
          message:
            "Cloud save configuration changed. Check again before syncing.",
        });
      const user = await tx.user.findUnique({
        where: { id: owner.userId },
        select: { enabled: true },
      });
      const access = await tx.collectionEntry.count({
        where: { gameId: owner.gameId, collection: { userId: owner.userId } },
      });
      if (!user?.enabled || !access)
        throw createError({
          statusCode: 403,
          message: "Game access was revoked during upload.",
        });
      const { gameId, userId, namespace } = owner;
      const head = await tx.cloudSaveHead.upsert({
        where: { owner: { gameId, userId, namespace } },
        create: { gameId, userId, namespace },
        update: {},
      });
      const previous = await tx.cloudSaveSnapshot.findUnique({
        where: { id: upload.id },
      });
      if (previous) {
        if (
          previous.headId !== head.id ||
          previous.checksum !== upload.checksum
        )
          throw createError({
            statusCode: 409,
            message: "Snapshot identity already used.",
          });
        // The response to a successful commit may have been lost. Keep the new
        // redundant object queued for reclamation, and return the original.
        return previous;
      }
      const revision = await advanceRevision(
        tx.cloudSaveHead,
        head.id,
        upload.expectedRevision,
      );
      const snapshot = await tx.cloudSaveSnapshot.create({
        data: {
          id: upload.id,
          headId: head.id,
          revision,
          definitionRevision: upload.definitionRevision,
          clientId: owner.clientId,
          kind: upload.kind,
          objectId,
          checksum: upload.checksum,
          contentHash: upload.contentHash,
          bytes,
        },
      });
      await tx.cloudSaveGarbage.delete({ where: { objectId } });
      const history = await tx.cloudSaveSnapshot.findMany({
        where: { headId: head.id },
        select: { id: true, revision: true, kind: true },
      });
      // The database trigger durably enqueues storage deletes, including cascades.
      await tx.cloudSaveSnapshot.deleteMany({
        where: { id: { in: snapshotIdsToPrune(history, limits.history) } },
      });
      return snapshot;
    });
  } catch (error) {
    // Leave the durable garbage record if deletion fails.
    try {
      if (await objectHandler.deleteAsSystem(objectId))
        await prisma.cloudSaveGarbage.deleteMany({ where: { objectId } });
    } catch {
      /* Retried by cleanSaveGarbage. */
    }
    throw error;
  }
}

export function snapshotSummary(snapshot: {
  id: string;
  revision: number;
  definitionRevision: number;
  clientId: string;
  kind: string;
  checksum: string;
  contentHash: string;
  bytes: number;
  createdAt: Date;
}) {
  const {
    id,
    revision,
    definitionRevision,
    clientId,
    kind,
    checksum,
    contentHash,
    bytes,
    createdAt,
  } = snapshot;
  return {
    id,
    revision,
    definitionRevision,
    clientId,
    kind,
    checksum,
    contentHash,
    bytes,
    createdAt,
  };
}
