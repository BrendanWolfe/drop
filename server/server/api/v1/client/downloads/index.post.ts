import { ArkErrors, type } from "arktype";
import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import prisma from "~/server/internal/db/database";
import {
  downloadRegistry,
  type DownloadReport,
} from "~/server/internal/downloads/registry";

const Body = type({
  reporterId: "string <= 100",
  sequence: "number.integer >= 0",
  downloads: type({
    sessionId: "string <= 200",
    gameId: "string <= 100",
    versionId: "string <= 100",
    state: "'Queued' | 'Downloading' | 'Paused' | 'Validating'",
    bytesDownloaded: "number.integer >= 0",
    bytesTotal: "number.integer >= 0",
    bytesPerSecond: "number.integer >= 0",
  })
    .array()
    .atMostLength(100),
});

export default defineClientEventHandler(
  async (h3, { fetchClient, fetchUser }) => {
    const body = Body(await readBody(h3));
    if (body instanceof ArkErrors)
      throw createError({ statusCode: 400, message: body.summary });
    const numericValues = [
      body.sequence,
      ...body.downloads.flatMap((entry) => [
        entry.bytesDownloaded,
        entry.bytesTotal,
        entry.bytesPerSecond,
      ]),
    ];
    if (!numericValues.every(Number.isSafeInteger))
      throw createError({ statusCode: 400, message: "Invalid byte counts" });
    const [client, user] = await Promise.all([fetchClient(), fetchUser()]);
    const games = await prisma.game.findMany({
      where: { id: { in: body.downloads.map((entry) => entry.gameId) } },
      select: { id: true, mName: true },
    });
    const titles = new Map(games.map((game) => [game.id, game.mName]));
    const seen = new Set<string>();
    const downloads = (body.downloads as DownloadReport[])
      .filter((entry) => {
        if (!titles.has(entry.gameId) || seen.has(entry.sessionId))
          return false;
        seen.add(entry.sessionId);
        return true;
      })
      .map((entry) => ({
        sessionId: entry.sessionId,
        gameId: entry.gameId,
        versionId: entry.versionId,
        state: entry.state,
        bytesDownloaded: entry.bytesDownloaded,
        bytesTotal: entry.bytesTotal,
        bytesPerSecond: entry.bytesPerSecond,
        clientId: client.id,
        userId: user.id,
        userName: user.displayName || user.username,
        deviceName: client.name,
        gameTitle: titles.get(entry.gameId)!,
      }));
    downloadRegistry.replace(
      client.id,
      body.reporterId,
      body.sequence,
      downloads,
    );
    return { ok: true };
  },
);
