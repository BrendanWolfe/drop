import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import prisma from "~/server/internal/db/database";
import { requireCloudSaveAccess } from "~/server/internal/cloud-saves/access";
import {
  parseLocations,
  parseNamespace,
} from "~/server/internal/cloud-saves/validation";
import {
  cleanSaveGarbage,
  saveLimits,
  snapshotSummary,
} from "~/server/internal/cloud-saves/storage";

export default defineClientEventHandler(
  async (event, { fetchUser, fetchClient }) => {
    const user = await fetchUser();
    const game = await requireCloudSaveAccess(
      getRouterParam(event, "gameid")!,
      user,
      await fetchClient(),
    );
    let namespace: string;
    try {
      namespace = parseNamespace(getQuery(event).namespace);
    } catch {
      throw createError({ statusCode: 400, message: "Invalid namespace." });
    }
    const head = await prisma.cloudSaveHead.findUnique({
      where: { owner: { gameId: game.id, userId: user.id, namespace } },
      include: { snapshots: { orderBy: { revision: "desc" } } },
    });
    await cleanSaveGarbage().catch(() => {});
    return {
      definitionRevision: game.cloudSaveRevision,
      locations: parseLocations(game.cloudSaveLocations).filter(
        (l) => l.platform === namespace,
      ),
      revision: head?.revision ?? 0,
      snapshots: head?.snapshots.map(snapshotSummary) ?? [],
      limits: await saveLimits(),
    };
  },
);
