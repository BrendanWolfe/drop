import { ArkErrors, type } from "arktype";
import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import { createDownloadManifestDetails } from "~/server/internal/library/manifest/index";
import { companionArchiveManifest } from "~/server/internal/library/manifest/companion-archive";
import { getAgeRestrictionFilter } from "~/server/internal/utils/ageRestrictions";
import prisma from "~/server/internal/db/database";
import {
  installerAssetDownload,
  installerAssetPrefix,
} from "~/server/internal/library/installer-assets";
import { castManifest } from "~/server/internal/library/manifest/utils";

const Query = type({
  version: "string",
  previous: "string?",
  refresh: "string?",
  archive: "string?",
  installer: "string?",
  installerAssets: "string?",
  installerFlow: "string?",
  id: "string?",
  companionFiles: "string?",
});

export default defineClientEventHandler(async (h3, { fetchUser }) => {
  const query = Query(getQuery(h3));
  if (query instanceof ArkErrors)
    throw createError({ statusCode: 400, message: query.summary });

  if (query.version.startsWith(installerAssetPrefix)) {
    const user = await fetchUser();
    const filter = await getAgeRestrictionFilter(user.id, user.admin);
    const asset =
      query.id &&
      (await prisma.installerAsset.findFirst({
        where: {
          id: query.version,
          version: { game: { id: query.id, ...filter } },
        },
        select: {
          id: true,
          manifest: true,
          filesystemKind: true,
          targetPath: true,
          filesystemEntry: true,
        },
      }));
    if (!asset)
      throw createError({
        statusCode: 404,
        message: "Installer companion not found",
      });
    return installerAssetDownload(
      asset.id,
      castManifest(asset.manifest),
      asset.filesystemKind
        ? {
            kind: asset.filesystemKind,
            target: asset.targetPath,
            archiveEntry: asset.filesystemEntry,
          }
        : undefined,
    );
  }

  const intent = await prisma.gameVersion.findUnique({
    where: { versionId: query.version },
    select: { locateAfterSetup: true, gameId: true },
  });
  if (
    intent?.locateAfterSetup &&
    (query.installerFlow !== "true" || query.id !== intent.gameId)
  )
    throw createError({
      statusCode: 409,
      message: "Update Drop Desktop to download this installer version.",
    });

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
  if (
    result.installerPackage &&
    query.installerAssets !== "true" &&
    (await prisma.installerAsset.count({ where: { versionId: query.version } }))
  )
    throw createError({
      statusCode: 409,
      message:
        "Update Drop Desktop to download this installer's separate companion files.",
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
  return intent?.locateAfterSetup ? { ...result, installerFlow: true } : result;
});
