import { ArkErrors, type } from "arktype";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import libraryManager from "~/server/internal/library";
import { listInstallerEntries } from "~/server/internal/library/installer-files";
import { browseInstallerZip } from "~/server/internal/library/installer-assets";
import { dropletInterface } from "~/server/internal/services/torrential/droplet-interface";

const Query = type({ id: "string", directory: "string?", archive: "string?" });

export default defineEventHandler(async (event) => {
  if (!(await aclManager.allowSystemACL(event, ["import:version:new"])))
    throw createError({ statusCode: 403 });

  const query = Query(getQuery(event));
  if (query instanceof ArkErrors)
    throw createError({ statusCode: 400, statusMessage: query.summary });
  const game = await prisma.game.findUnique({
    where: { id: query.id },
    select: {
      libraryId: true,
      libraryPath: true,
      isEmulated: true,
      type: true,
    },
  });
  const library = game && libraryManager.getLibrary(game.libraryId);
  if (!game || !library || game.type !== "Game" || game.isEmulated)
    throw createError({
      statusCode: 404,
      statusMessage: "Game library not available",
    });

  const directory = query.directory ?? (query.archive ? "" : game.libraryPath);
  try {
    return {
      directory,
      entries: query.archive
        ? await browseInstallerZip(
            library.filesystemRoot(),
            query.archive,
            directory,
            (file) => dropletInterface.listFiles(file),
          )
        : await listInstallerEntries(library.filesystemRoot(), directory),
    };
  } catch {
    throw createError({
      statusCode: 400,
      statusMessage: "Could not open this library folder",
    });
  }
});
