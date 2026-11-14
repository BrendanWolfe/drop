import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import { parseLocations } from "~/server/internal/cloud-saves/validation";

export default defineEventHandler(async (event) => {
  if (!(await aclManager.allowSystemACL(event, ["game:read"])))
    throw createError({ statusCode: 403 });
  const game = await prisma.game.findUnique({
    where: { id: getRouterParam(event, "id") },
    select: {
      cloudSavesEnabled: true,
      cloudSaveRevision: true,
      cloudSaveConfig: { select: { locations: true } },
    },
  });
  if (!game) throw createError({ statusCode: 404 });
  return {
    enabled: game.cloudSavesEnabled,
    revision: game.cloudSaveRevision,
    locations: parseLocations(game.cloudSaveConfig?.locations ?? []),
  };
});
