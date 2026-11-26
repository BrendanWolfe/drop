import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import prisma from "~/server/internal/db/database";

export default defineClientEventHandler(async (h3) => {
  const id = getRouterParam(h3, "id");
  const version = getRouterParam(h3, "versionid");
  if (!id || !version)
    throw createError({
      statusCode: 400,
      statusMessage: "Missing id or version in route params",
    });

  const gameVersion = await prisma.gameVersion.findUnique({
    where: {
      versionId: version,
    },
    include: {
      installerAssets: {
        select: { id: true, name: true, targetPath: true, filesystemKey: true },
        orderBy: { name: "asc" },
      },
      launches: {
        include: {
          emulator: {
            include: {
              gameVersion: {
                select: {
                  game: {
                    select: {
                      id: true,
                    },
                  },
                },
              },
            },
          },
        },
      },
      setups: {
        include: {
          emulator: {
            select: {
              launchId: true,
              versionId: true,
              gameVersion: { select: { gameId: true } },
            },
          },
        },
      },
    },
  });

  if (!gameVersion || gameVersion.gameId !== id)
    throw createError({
      statusCode: 404,
      statusMessage: "Game version not found",
    });

  const gameVersionMapped = {
    ...gameVersion,
    versionPath:
      gameVersion.versionPath ??
      gameVersion.displayName ??
      gameVersion.versionId,
    installerBundleId: undefined,
    companionArchiveManifest: undefined,
    locateAfterSetup:
      gameVersion.locateAfterSetup || !!gameVersion.installerBundleId,
    installerLibraryInputs: gameVersion.installerLibraryInputs ?? [],
    // Retain the old wire name for bundled support files read by older clients.
    installerCompanionFiles: gameVersion.installerBundleId
      ? gameVersion.fileList.slice(1)
      : undefined,
    setups: gameVersion.setups.map(({ emulator, ...setup }) => ({
      ...setup,
      emulator: emulator && {
        launchId: emulator.launchId,
        versionId: emulator.versionId,
        gameId: emulator.gameVersion.gameId,
      },
    })),
    launches: gameVersion.launches.map((launch) => ({
      ...launch,
      emulator: launch.emulator
        ? {
            ...launch.emulator,
            gameVersion: undefined,
            gameId: launch.emulator.gameVersion.game.id,
          }
        : undefined,
    })),
  };

  return gameVersionMapped;
});
