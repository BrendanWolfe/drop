import { ArkErrors } from "arktype";
import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import prisma from "~/server/internal/db/database";
import { recordDownloadActivity } from "~/server/internal/activity/history";
import {
  DownloadActivityBody,
  validTimestamp,
} from "~/server/internal/activity/validation";

export default defineClientEventHandler(
  async (h3, { fetchClient, fetchUser }) => {
    const body = DownloadActivityBody(await readBody(h3));
    if (body instanceof ArkErrors)
      throw createError({ statusCode: 400, message: body.summary });
    if (
      !validTimestamp(body.occurredAt) ||
      ![body.bytesDownloaded, body.bytesTotal].every(Number.isSafeInteger)
    )
      throw createError({
        statusCode: 400,
        message: "Invalid download activity",
      });
    const [client, user, game] = await Promise.all([
      fetchClient(),
      fetchUser(),
      prisma.game.findUnique({
        where: { id: body.gameId },
        select: { mName: true },
      }),
    ]);
    const previous = !game
      ? await prisma.downloadHistory.findUnique({
          where: {
            clientId_sessionId: {
              clientId: client.id,
              sessionId: body.sessionId,
            },
          },
          select: { gameId: true, gameTitle: true },
        })
      : null;
    if (!game && previous?.gameId !== body.gameId)
      throw createError({ statusCode: 404 });
    await prisma.$transaction((tx) =>
      recordDownloadActivity(
        tx.downloadHistory,
        {
          clientId: client.id,
          userId: user.id,
          userName: user.displayName || user.username,
          deviceName: client.name,
          gameTitle: game?.mName ?? previous!.gameTitle,
        },
        body,
      ),
    );
    return { ok: true };
  },
);
