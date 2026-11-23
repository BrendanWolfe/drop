import { ArkErrors, type } from "arktype";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import libraryManager from "~/server/internal/library";
import {
  listInputDirectory,
  validLibraryInputPath,
} from "~/server/internal/library/installer-input-paths";
import {
  findInstallerSource,
  installerSourceFiles,
  installerSourceFilter,
} from "~/server/internal/library/installer-library-inputs";

const Query = type({
  owner: "string.uuid",
  source: "string.uuid?",
  version: "string.uuid?",
  directory: type("string <= 1024").default(""),
  search: type("string <= 200").default(""),
});

export default defineEventHandler(async (event) => {
  if (!(await aclManager.allowSystemACL(event, ["import:version:new"])))
    throw createError({ statusCode: 403 });
  const query = Query(getQuery(event));
  if (query instanceof ArkErrors || query.source === query.owner)
    throw createError({
      statusCode: 400,
      message: "Choose another library game.",
    });
  if (!query.source) {
    const games = await prisma.game.findMany({
      where: {
        id: { not: query.owner },
        type: "Game",
        mName: { contains: query.search, mode: "insensitive" },
        versions: { some: installerSourceFilter },
      },
      select: { id: true, mName: true },
      orderBy: [{ mName: "asc" }, { id: "asc" }],
      take: 50,
    });
    return { games };
  }
  if (!query.version) {
    const versions = await prisma.gameVersion.findMany({
      where: {
        ...installerSourceFilter,
        gameId: query.source,
        game: { type: "Game" },
      },
      select: { versionId: true, displayName: true, versionPath: true },
      orderBy: { versionIndex: "desc" },
    });
    return { versions };
  }
  if (query.directory && !validLibraryInputPath(query.directory))
    throw createError({
      statusCode: 400,
      message: "Choose a folder from the selected game version.",
    });
  const source = await findInstallerSource(query.source, query.version);
  if (!source)
    throw createError({
      statusCode: 404,
      message: "Game version not available.",
    });
  const files = await installerSourceFiles(
    source,
    libraryManager.listInstallerArchive.bind(libraryManager),
  );
  const entries = listInputDirectory(files, query.directory, query.search);
  return { entries: entries.slice(0, 200), more: entries.length > 200 };
});
