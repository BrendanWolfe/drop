import { type } from "arktype";
import { readDropValidatedBody, throwingArktype } from "~/server/arktype";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import { castManifest } from "~/server/internal/library/manifest/utils";

const UpdateVersionOrder = type({
  versions: "string[]",
}).configure(throwingArktype);

export default defineEventHandler(async (h3) => {
  const allowed = await aclManager.allowSystemACL(h3, ["game:version:update"]);
  if (!allowed) throw createError({ statusCode: 403 });

  const body = await readDropValidatedBody(h3, UpdateVersionOrder);
  const gameId = getRouterParam(h3, "id")!;
  // We expect an array of the version names for this game
  const unsortedVersions = await prisma.gameVersion.findMany({
    where: {
      versionId: { in: body.versions },
    },
    select: {
      versionId: true,
      versionIndex: true,
      delta: true,
      dropletManifest: true,
      launches: { select: { platform: true } },
    },
  });

  const versions = body.versions
    .map((e) => unsortedVersions.find((v) => v.versionId === e))
    .filter((e) => e !== undefined);

  if (versions.length !== unsortedVersions.length)
    throw createError({
      statusCode: 500,
      statusMessage: "Sorting versions yielded less results, somehow.",
    });

  // Validate the new order
  const has: { [key: string]: boolean } = {};
  let archiveBase = false;
  for (const version of versions) {
    if (version.delta && archiveBase)
      throw createError({
        statusCode: 400,
        statusMessage:
          "Delta versions cannot be ordered on top of archive installations.",
      });
    if (!version.delta)
      archiveBase =
        castManifest(version.dropletManifest).version === "archive-v1";
    for (const versionPlatform of version.launches.map((v) => v.platform)) {
      if (version.delta && !has[versionPlatform])
        throw createError({
          statusCode: 400,
          statusMessage: `"${version.versionId}" requires a base version to apply the delta to for platform ${versionPlatform}.`,
        });
      has[versionPlatform] = true;
    }
  }

  await prisma.$transaction(
    versions.map((version, versionIndex) =>
      prisma.gameVersion.updateMany({
        where: {
          gameId: gameId,
          versionId: version.versionId,
        },
        data: {
          versionIndex: versionIndex,
        },
      }),
    ),
  );

  return versions.map((v) => v.versionId);
});
