import type { Prisma } from "~/prisma/client/client";
import prisma from "../db/database";
import { createDownloadManifestDetails } from "./manifest";
import {
  hasLibraryInput,
  installedFilePaths,
  type LibraryInput,
} from "./installer-input-paths";

export const installerSourceFilter = {
  installerBundleId: null,
  onlySetup: false,
} satisfies Prisma.GameVersionWhereInput;

export async function findInstallerSource(gameId: string, versionId: string) {
  return prisma.gameVersion.findFirst({
    where: {
      ...installerSourceFilter,
      versionId,
      gameId,
      game: { type: "Game" },
    },
    select: {
      gameId: true,
      versionId: true,
      displayName: true,
      versionPath: true,
      fileList: true,
      delta: true,
      game: { select: { mName: true, libraryId: true, libraryPath: true } },
    },
  });
}

export async function installerSourceFiles(
  source: NonNullable<Awaited<ReturnType<typeof findInstallerSource>>>,
  listArchive: (
    libraryId: string,
    gamePath: string,
    versionPath: string,
  ) => Promise<string[]>,
) {
  if (
    source.fileList.length === 1 &&
    source.fileList[0] === ".drop-archive" &&
    source.versionPath
  )
    return installedFilePaths(
      await listArchive(
        source.game.libraryId,
        source.game.libraryPath,
        source.versionPath,
      ),
    );
  if (source.delta)
    return installedFilePaths(
      Object.keys(
        (await createDownloadManifestDetails(source.versionId)).fileList,
      ),
    );
  return installedFilePaths(source.fileList);
}

export async function validateInstallerInputs(
  owner: string,
  inputs: LibraryInput[],
  listArchive: Parameters<typeof installerSourceFiles>[1],
) {
  const seen = new Set<string>();
  const sources = new Map<
    string,
    Awaited<ReturnType<typeof findInstallerSource>>
  >();
  const paths = new Map<string, Set<string>>();
  const result = [];
  for (const input of inputs) {
    const key = JSON.stringify([input.versionId, input.kind, input.path]);
    if (input.gameId === owner || seen.has(key))
      throw createError({
        statusCode: 400,
        message: "Choose distinct files or folders from another library game.",
      });
    seen.add(key);
    if (!sources.has(input.versionId))
      sources.set(
        input.versionId,
        await findInstallerSource(input.gameId, input.versionId),
      );
    const source = sources.get(input.versionId);
    if (!source || source.gameId !== input.gameId)
      throw createError({
        statusCode: 400,
        message: "The selected library game version is no longer available.",
      });
    if (input.kind !== "folder" || input.path !== "") {
      if (!paths.has(input.versionId))
        paths.set(
          input.versionId,
          await installerSourceFiles(source, listArchive),
        );
      if (!hasLibraryInput(paths.get(input.versionId)!, input))
        throw createError({
          statusCode: 400,
          message: "The selected file or folder is not in that version.",
        });
    }
    result.push({
      ...input,
      gameName: source.game.mName,
      versionName: source.displayName || source.versionPath || source.versionId,
    });
  }
  return result;
}
