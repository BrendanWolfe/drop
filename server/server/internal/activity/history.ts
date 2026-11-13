import type { Prisma } from "../../../prisma/client/client.ts";
import type { DownloadActivityBody } from "./validation.ts";

type Writer = {
  upsert: (args: Prisma.DownloadHistoryUpsertArgs) => Promise<unknown>;
  updateMany: (args: Prisma.DownloadHistoryUpdateManyArgs) => Promise<unknown>;
};

export async function recordDownloadActivity(
  writer: Writer,
  identity: {
    clientId: string;
    userId: string;
    userName: string;
    deviceName: string;
    gameTitle: string;
  },
  body: typeof DownloadActivityBody.infer,
  now = Date.now(),
) {
  const at = new Date(Math.min(body.occurredAt, now));
  const terminal = ["Completed", "Cancelled", "Failed"].includes(body.state);
  const key = { clientId: identity.clientId, sessionId: body.sessionId };
  const progress = {
    bytesDownloaded: BigInt(body.bytesDownloaded),
    bytesTotal: BigInt(body.bytesTotal),
  };
  // Conditional updates make retries idempotent and terminal outcomes immutable.
  await writer.upsert({
    where: { clientId_sessionId: key },
    create: {
      ...identity,
      ...key,
      gameId: body.gameId,
      versionId: body.versionId,
      platform: body.platform,
      state: body.state,
      queuedAt: at,
      startedAt: body.state === "Downloading" ? at : null,
      endedAt: terminal ? at : null,
      updatedAt: at,
      ...progress,
    },
    update: {},
  });
  await writer.updateMany({
    where: {
      ...key,
      endedAt: null,
      // An explicit terminal result wins even if the device clock moved back.
      ...(terminal ? {} : { updatedAt: { lte: at } }),
      ...(body.state === "Queued" ? { startedAt: null } : {}),
    },
    data: {
      state: body.state,
      updatedAt: at,
      ...(terminal ? { endedAt: at } : {}),
      ...progress,
    },
  });
  if (body.state === "Downloading")
    await writer.updateMany({
      where: { ...key, startedAt: null, endedAt: null },
      data: { startedAt: at },
    });
}
