import { readBody, createError } from "h3";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import { parseConfiguration } from "~/server/internal/cloud-saves/validation";

export default defineEventHandler(async (event) => {
  if (!(await aclManager.allowSystemACL(event, ["game:update"])))
    throw createError({ statusCode: 403 });
  let config;
  try {
    config = parseConfiguration(await readBody(event));
  } catch (error) {
    throw createError({ statusCode: 400, message: (error as Error).message });
  }
  const id = getRouterParam(event, "id")!;
  await prisma.$transaction(async (tx) => {
    const { count } = await tx.game.updateMany({
      where: { id, type: "Game", cloudSaveRevision: config.revision },
      data: {
        cloudSavesEnabled: config.enabled,
        cloudSaveRevision: { increment: 1 },
      },
    });
    if (!count)
      throw createError({
        statusCode: 409,
        message: "Game configuration changed. Reload and try again.",
      });
    await tx.cloudSaveConfig.upsert({
      where: { gameId: id },
      create: { gameId: id, locations: config.locations },
      update: { locations: config.locations },
    });
  });
  return {
    cloudSavesEnabled: config.enabled,
    cloudSaveRevision: config.revision + 1,
  };
});
