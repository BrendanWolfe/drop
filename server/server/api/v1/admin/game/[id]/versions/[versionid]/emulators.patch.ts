import { type } from "arktype";
import { readDropValidatedBody, throwingArktype } from "~/server/arktype";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import { validateEmulatorBindings } from "~/server/internal/library/emulator-bindings";

const Body = type({
  launches: type({
    launchId: "string",
    emulatorId: "string | null",
    previousEmulatorId: "string | null",
  })
    .array()
    .atMostLength(256),
}).configure(throwingArktype);

export default defineEventHandler(async (h3) => {
  if (!(await aclManager.allowSystemACL(h3, ["game:version:update"])))
    throw createError({ statusCode: 403 });
  const body = await readDropValidatedBody(h3, Body);
  const gameId = getRouterParam(h3, "id")!;
  const versionId = getRouterParam(h3, "versionid")!;

  return prisma.$transaction(async (tx) => {
    const version = await tx.gameVersion.findFirst({
      where: { versionId, gameId },
      include: {
        game: { select: { type: true, isEmulated: true } },
        launches: true,
      },
    });
    if (!version)
      throw createError({
        statusCode: 404,
        message: "Game version not found.",
      });
    if (version.game.type !== "Game" || version.onlySetup)
      throw createError({
        statusCode: 400,
        message: "Only game launch options can use emulators.",
      });
    const emulators = await tx.launchConfiguration.findMany({
      where: {
        launchId: {
          in: body.launches.flatMap((launch) =>
            launch.emulatorId ? [launch.emulatorId] : [],
          ),
        },
      },
      include: {
        gameVersion: { include: { game: { select: { type: true } } } },
      },
    });
    const error = validateEmulatorBindings(
      version.launches,
      body.launches,
      emulators,
      version.game.isEmulated,
    );
    if (error) throw createError({ statusCode: 409, message: error });
    for (const launch of body.launches) {
      const updated = await tx.launchConfiguration.updateMany({
        where: {
          launchId: launch.launchId,
          versionId,
          emulatorId: launch.previousEmulatorId,
        },
        data: { emulatorId: launch.emulatorId },
      });
      if (!updated.count)
        throw createError({
          statusCode: 409,
          message:
            "The emulator requirements changed. Reopen the editor and try again.",
        });
    }
    // Only emulator links change: version IDs, ROM paths, commands and depots stay intact.
    const launches = await tx.launchConfiguration.findMany({
      where: { versionId },
      include: {
        emulator: {
          include: {
            gameVersion: {
              select: {
                versionId: true,
                displayName: true,
                versionPath: true,
                game: {
                  select: { id: true, mName: true, mIconObjectId: true },
                },
              },
            },
          },
        },
        emulations: { select: { launchId: true } },
      },
    });
    return { launches };
  });
});
