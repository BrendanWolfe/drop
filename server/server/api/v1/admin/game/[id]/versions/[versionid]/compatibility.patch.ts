import { type } from "arktype";
import { readDropValidatedBody, throwingArktype } from "~/server/arktype";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";

const Compatibility = type({
  kind: "'launch' | 'setup'",
  configId: "string",
  protonCompatible: "boolean",
}).configure(throwingArktype);

export default defineEventHandler(async (h3) => {
  if (!(await aclManager.allowSystemACL(h3, ["game:version:update"])))
    throw createError({ statusCode: 403 });

  const body = await readDropValidatedBody(h3, Compatibility);
  const gameId = getRouterParam(h3, "id")!;
  const versionId = getRouterParam(h3, "versionid")!;
  const where = {
    versionId,
    gameVersion: { gameId },
    platform: "Windows" as const,
  };
  const data = { protonCompatible: body.protonCompatible };
  const result =
    body.kind === "launch"
      ? await prisma.launchConfiguration.updateMany({
          where: { ...where, launchId: body.configId },
          data,
        })
      : await prisma.setupConfiguration.updateMany({
          where: { ...where, setupId: body.configId },
          data,
        });
  if (!result.count)
    throw createError({
      statusCode: 404,
      statusMessage: "Windows launch or setup not found",
    });
  return data;
});
