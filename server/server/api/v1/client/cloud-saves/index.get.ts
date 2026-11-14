import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import prisma from "~/server/internal/db/database";
import { getAgeRestrictionFilter } from "~/server/internal/utils/ageRestrictions";
import { cleanSaveGarbage } from "~/server/internal/cloud-saves/storage";
import { requireCloudSaveCapability } from "~/server/internal/cloud-saves/access";

export default defineClientEventHandler(
  async (_event, { fetchUser, fetchClient }) => {
    requireCloudSaveCapability(await fetchClient());
    const user = await fetchUser();
    if (!user.enabled) throw createError({ statusCode: 403 });
    const ageFilter = await getAgeRestrictionFilter(user.id, user.admin);
    const games = await prisma.game.findMany({
      where: {
        type: "Game",
        cloudSavesEnabled: true,
        collections: { some: { collection: { userId: user.id } } },
        AND: ageFilter ? [ageFilter] : [],
      },
      select: { id: true, cloudSaveRevision: true },
    });
    await cleanSaveGarbage().catch(() => {});
    return games;
  },
);
