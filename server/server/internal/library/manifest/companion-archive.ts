import { Prisma } from "~/prisma/client/client";
import prisma from "../../db/database";
import type { DownloadManifestDetails } from "./index";
import { castManifest } from "./utils";

export const companionArchiveSuffix = "-companion-files";

const preparing = new Map<string, Promise<DownloadManifestDetails>>();

// The compressed download is monolithic. Index the entries through the same
// depot backend used for non-archive installs, then request only relevant chunks.
export function companionArchiveManifest(gameId: string, versionId: string) {
  const key = `${gameId}/${versionId}`;
  const pending = preparing.get(key);
  if (pending) return pending;
  const result = prepare(gameId, versionId).finally(() =>
    preparing.delete(key),
  );
  preparing.set(key, result);
  return result;
}

async function prepare(
  gameId: string,
  versionId: string,
): Promise<DownloadManifestDetails> {
  const version = await prisma.gameVersion.findFirst({
    where: {
      gameId,
      versionId,
      installerBundleId: null,
      onlySetup: false,
      game: { type: "Game" },
    },
    select: {
      versionPath: true,
      companionArchiveManifest: true,
      game: { select: { libraryId: true, libraryPath: true } },
    },
  });
  if (!version?.versionPath)
    throw createError({
      statusCode: 409,
      message: "This archive version cannot provide individual files.",
    });

  let raw = version.companionArchiveManifest;
  if (!raw) {
    const { default: library } = await import("..");
    const generated = await library.indexCompanionArchive(
      version.game.libraryId,
      version.game.libraryPath,
      version.versionPath,
    );
    // Persist before returning a synthetic depot version that references it.
    await prisma.gameVersion.updateMany({
      where: { versionId, companionArchiveManifest: { equals: Prisma.DbNull } },
      data: { companionArchiveManifest: generated },
    });
    raw = (
      await prisma.gameVersion.findUniqueOrThrow({
        where: { versionId },
        select: { companionArchiveManifest: true },
      })
    ).companionArchiveManifest;
  }
  const manifest = castManifest(raw);
  if (manifest?.version !== "2")
    throw createError({
      statusCode: 500,
      message: "Invalid companion archive index.",
    });
  const depotVersion = versionId + companionArchiveSuffix;
  // The depot keeps archive entry names verbatim when reading from libarchive.
  // On the client these are normal installation-relative paths.
  const chunks = Object.fromEntries(
    Object.entries(manifest.chunks).map(([id, chunk]) => [
      id,
      {
        ...chunk,
        files: chunk.files.map((file) => ({
          ...file,
          filename: file.filename.replace(/^\.\//u, ""),
        })),
      },
    ]),
  );
  return {
    manifests: { [depotVersion]: { ...manifest, chunks } },
    fileList: Object.fromEntries(
      Object.values(chunks).flatMap((chunk) =>
        chunk.files.map((file) => [file.filename, depotVersion]),
      ),
    ),
    installSize: manifest.size,
    downloadSize: manifest.size,
  };
}
