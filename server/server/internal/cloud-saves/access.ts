import prisma from "../db/database.ts";
import { getAgeRestrictionFilter } from "../utils/ageRestrictions.ts";
import { createError } from "h3";
import { parseLocations } from "./validation.ts";

export function requireCloudSaveCapability(client: { capabilities: string[] }) {
  if (!client.capabilities.includes("CloudSaves"))
    throw createError({
      statusCode: 403,
      message: "Cloud saves capability not granted to this device.",
    });
}

export async function requireCloudSaveAccess(
  gameId: string,
  user: { id: string; enabled: boolean; admin: boolean },
  client?: { capabilities: string[] },
) {
  if (client) requireCloudSaveCapability(client);
  if (!user.enabled)
    throw createError({ statusCode: 403, message: "Account disabled." });
  const ageFilter = await getAgeRestrictionFilter(user.id, user.admin);
  const game = await prisma.game.findFirst({
    where: {
      id: gameId,
      type: "Game",
      collections: { some: { collection: { userId: user.id } } },
      AND: ageFilter ? [ageFilter] : [],
    },
    select: {
      id: true,
      cloudSavesEnabled: true,
      cloudSaveRevision: true,
      cloudSaveConfig: { select: { locations: true } },
    },
  });
  if (!game)
    throw createError({
      statusCode: 404,
      message: "Game not in your library.",
    });
  if (!game.cloudSavesEnabled)
    throw createError({
      statusCode: 403,
      message: "Cloud saves are not enabled for this game.",
    });
  return {
    id: game.id,
    cloudSavesEnabled: game.cloudSavesEnabled,
    cloudSaveRevision: game.cloudSaveRevision,
    cloudSaveLocations: parseLocations(game.cloudSaveConfig?.locations ?? []),
  };
}
