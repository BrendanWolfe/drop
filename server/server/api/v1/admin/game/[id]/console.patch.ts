import { type } from "arktype";
import { readDropValidatedBody, throwingArktype } from "~/server/arktype";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import { consoleTypeIdSchema } from "~/server/internal/library/console-types";

// The shared body reader removes nulls, so restore null for a cleared selection.
const PatchConsole = type({
  consoleTypeId: consoleTypeIdSchema.default(null),
}).configure(throwingArktype);

export default defineEventHandler(async (h3) => {
  const allowed = await aclManager.allowSystemACL(h3, ["game:update"]);
  if (!allowed) throw createError({ statusCode: 403 });

  const { consoleTypeId } = await readDropValidatedBody(h3, PatchConsole);
  const id = getRouterParam(h3, "id")!;
  const game = await prisma.game.findUnique({
    where: { id },
    select: { type: true, isEmulated: true },
  });
  if (!game) throw createError({ statusCode: 404, message: "Game not found" });
  if (game.type !== "Game" || !game.isEmulated)
    throw createError({
      statusCode: 400,
      message: "Only emulated games can have a console type.",
    });
  if (
    consoleTypeId &&
    !(await prisma.consoleType.findUnique({ where: { id: consoleTypeId } }))
  )
    throw createError({ statusCode: 400, message: "Console type not found." });

  const result = await prisma.game.updateMany({
    where: { id, type: "Game", isEmulated: true },
    data: { consoleTypeId },
  });
  if (!result.count)
    throw createError({
      statusCode: 409,
      message: "The game changed. Reload and try again.",
    });
  return { consoleTypeId };
});
