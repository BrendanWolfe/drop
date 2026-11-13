import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import { getAgeRestrictionFilter } from "~/server/internal/utils/ageRestrictions";

export default defineEventHandler(async (h3) => {
  const user = await aclManager.getUserACL(h3, ["store:read"]);
  if (!user) throw createError({ statusCode: 403 });
  const ageFilter = await getAgeRestrictionFilter(user.id, user.admin);

  return prisma.consoleType.findMany({
    where: {
      games: { some: { ...ageFilter, type: "Game", isEmulated: true } },
    },
    select: { id: true, name: true },
    orderBy: { name: "asc" },
  });
});
