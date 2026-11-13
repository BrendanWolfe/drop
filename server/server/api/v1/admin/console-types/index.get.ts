import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";

export default defineEventHandler(async (h3) => {
  const allowed = await aclManager.allowSystemACL(h3, [
    "import:game:new",
    "game:read",
    "game:update",
  ]);
  if (!allowed) throw createError({ statusCode: 403 });

  return prisma.consoleType.findMany({
    select: { id: true, name: true },
    orderBy: { name: "asc" },
  });
});
