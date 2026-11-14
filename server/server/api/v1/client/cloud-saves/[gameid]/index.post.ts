import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import { requireCloudSaveAccess } from "~/server/internal/cloud-saves/access";
import {
  parseLocations,
  parseNamespace,
  parseUploadHeaders,
} from "~/server/internal/cloud-saves/validation";
import {
  snapshotSummary,
  uploadSnapshot,
} from "~/server/internal/cloud-saves/storage";

export default defineClientEventHandler(
  async (event, { fetchUser, fetchClient, clientId }) => {
    const user = await fetchUser();
    const game = await requireCloudSaveAccess(
      getRouterParam(event, "gameid")!,
      user,
      await fetchClient(),
    );
    let namespace: string;
    let upload;
    try {
      namespace = parseNamespace(getQuery(event).namespace);
      upload = parseUploadHeaders((key) => getHeader(event, key));
    } catch (error) {
      throw createError({ statusCode: 400, message: (error as Error).message });
    }
    if (
      upload.definitionRevision !== game.cloudSaveRevision ||
      !parseLocations(game.cloudSaveLocations).some(
        (l) => l.platform === namespace,
      )
    )
      throw createError({
        statusCode: 409,
        message: "Save configuration changed or platform not configured.",
      });
    return snapshotSummary(
      await uploadSnapshot(
        event.node.req,
        { gameId: game.id, userId: user.id, namespace, clientId },
        upload,
      ),
    );
  },
);
