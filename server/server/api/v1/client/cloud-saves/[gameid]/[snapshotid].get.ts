import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import prisma from "~/server/internal/db/database";
import objectHandler from "~/server/internal/objects";
import { requireCloudSaveAccess } from "~/server/internal/cloud-saves/access";

export default defineClientEventHandler(
  async (event, { fetchUser, fetchClient }) => {
    const user = await fetchUser();
    const game = await requireCloudSaveAccess(
      getRouterParam(event, "gameid")!,
      user,
      await fetchClient(),
    );
    const snapshot = await prisma.cloudSaveSnapshot.findFirst({
      where: {
        id: getRouterParam(event, "snapshotid"),
        head: { gameId: game.id, userId: user.id },
      },
    });
    if (!snapshot) throw createError({ statusCode: 404 });
    const data = await objectHandler.fetch(snapshot.objectId);
    if (!data)
      throw createError({
        statusCode: 404,
        message: "Snapshot archive missing.",
      });
    setHeader(event, "Content-Type", "application/octet-stream");
    setHeader(event, "Content-Length", snapshot.bytes);
    setHeader(event, "X-Archive-Sha256", snapshot.checksum);
    return data;
  },
);
