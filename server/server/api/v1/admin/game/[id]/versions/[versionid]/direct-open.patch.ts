import { type } from "arktype";
import { readDropValidatedBody, throwingArktype } from "~/server/arktype";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import {
  EmulatorProfile,
  validateDirectOpenCommand,
} from "~/server/internal/library/emulator-profile";

const DirectOpen = type({ launchId: "string", command: "string" }).configure(
  throwingArktype,
);

export default defineEventHandler(async (h3) => {
  if (!(await aclManager.allowSystemACL(h3, ["game:version:update"])))
    throw createError({ statusCode: 403 });
  const body = await readDropValidatedBody(h3, DirectOpen);
  const error = validateDirectOpenCommand(body.command);
  if (error) throw createError({ statusCode: 400, message: error });
  const where = {
    launchId: body.launchId,
    versionId: getRouterParam(h3, "versionid")!,
    gameVersion: {
      gameId: getRouterParam(h3, "id")!,
      game: { type: "Emulator" as const },
    },
  };
  return await prisma.$transaction(async (tx) => {
    const launch = await tx.launchConfiguration.findFirst({ where });
    if (!launch)
      throw createError({
        statusCode: 404,
        statusMessage: "Emulator launch not found",
      });
    const existing =
      launch.emulatorProfile == null
        ? {
            mode: launch.command.includes("{rom}") ? "cli" : "ui",
            configFiles: [],
          }
        : launch.emulatorProfile;
    const profile = EmulatorProfile(existing);
    if (profile instanceof type.errors)
      throw createError({
        statusCode: 400,
        message: "Invalid emulator profile",
      });
    const command = body.command.trim();
    if (command) profile.directOpenCommand = command;
    else delete profile.directOpenCommand;
    await tx.launchConfiguration.updateMany({
      where,
      data: { emulatorProfile: profile },
    });
    return profile;
  });
});
