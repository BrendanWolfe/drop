import { ArkErrors, type } from "arktype";
import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import { createDownloadManifestDetails } from "~/server/internal/library/manifest/index";
import { companionArchiveManifest } from "~/server/internal/library/manifest/companion-archive";
import { getAgeRestrictionFilter } from "~/server/internal/utils/ageRestrictions";
import prisma from "~/server/internal/db/database";

const Query = type({
  version: "string",
  previous: "string?",
  refresh: "string?",
  archive: "string?",
  installer: "string?",
  id: "string?",
  companionFiles: "string?",
});

export default defineClientEventHandler(async (h3, { fetchUser }) => {
  const query = Query(getQuery(h3));
  if (query instanceof ArkErrors)
    throw createError({ statusCode: 400, message: query.summary });

  const result = await createDownloadManifestDetails(
    query.version,
    query.previous,
    query.refresh == "true",
  );
  if (result.installerPackage && query.installer !== "true")
    throw createError({
      statusCode: 409,
      message: "Update Drop Desktop to download this installer package.",
    });
  const isArchive = Object.values(result.manifests).some(
    (manifest) => manifest.version === "archive-v1",
  );
  if (isArchive && query.companionFiles === "true" && query.id) {
    const user = await fetchUser();
    const filter = await getAgeRestrictionFilter(user.id, user.admin);
    if (
      filter &&
      !(await prisma.game.count({ where: { id: query.id, ...filter } }))
    )
      throw createError({ statusCode: 404, message: "Game not found" });
    return companionArchiveManifest(query.id, query.version);
  }
  if (query.archive !== "true" && isArchive)
    throw createError({
      statusCode: 409,
      message:
        "This version requires a desktop client with archive installation support. Update Drop Desktop and retry.",
    });
  return result;
});
