import { ArkErrors } from "arktype";
import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import prisma from "~/server/internal/db/database";
import {
  ErrorReportBody,
  redactReport,
  validTimestamp,
} from "~/server/internal/activity/validation";

export default defineClientEventHandler(
  async (h3, { fetchClient, fetchUser }) => {
    const body = ErrorReportBody(await readBody(h3));
    if (body instanceof ArkErrors)
      throw createError({ statusCode: 400, message: body.summary });
    if (!validTimestamp(body.occurredAt))
      throw createError({ statusCode: 400 });
    const [client, user] = await Promise.all([fetchClient(), fetchUser()]);
    // Bound storage from malfunctioning clients, while allowing idempotent retries.
    const existing = await prisma.clientErrorReport.findUnique({
      where: {
        clientId_reportId: { clientId: client.id, reportId: body.reportId },
      },
      select: { id: true },
    });
    if (existing) return { ok: true };
    const recent = await prisma.clientErrorReport.count({
      where: {
        clientId: client.id,
        receivedAt: { gte: new Date(Date.now() - 60_000) },
      },
    });
    if (recent >= 30) throw createError({ statusCode: 429 });
    const game = body.gameId
      ? await prisma.game.findUnique({
          where: { id: body.gameId },
          select: { mName: true },
        })
      : null;
    await prisma.clientErrorReport.upsert({
      where: {
        clientId_reportId: { clientId: client.id, reportId: body.reportId },
      },
      create: {
        reportId: body.reportId,
        source: body.source,
        gameId: body.gameId,
        clientVersion: body.clientVersion,
        clientId: client.id,
        userId: user.id,
        userName: user.displayName || user.username,
        deviceName: client.name,
        gameTitle: game?.mName,
        message: redactReport(body.message),
        details: redactReport(body.details),
        occurredAt: new Date(Math.min(body.occurredAt, Date.now())),
      },
      update: {},
    });
    return { ok: true };
  },
);
