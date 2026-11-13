import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import { historyPage } from "~/server/internal/activity/validation";

export default defineEventHandler(async (h3) => {
  if (!(await aclManager.allowSystemACL(h3, ["errors:read"])))
    throw createError({ statusCode: 403 });
  const page = historyPage(getQuery(h3));
  if (!page) throw createError({ statusCode: 400 });
  const search = String(getQuery(h3).q ?? "")
    .trim()
    .slice(0, 200);
  const rows = await prisma.clientErrorReport.findMany({
    where: search
      ? {
          OR: ["userName", "deviceName", "gameTitle", "message"].map(
            (field) => ({
              [field]: { contains: search, mode: "insensitive" as const },
            }),
          ),
        }
      : {},
    skip: page.skip,
    take: page.take + 1,
    orderBy: [{ receivedAt: "desc" }, { id: "desc" }],
  });
  return { items: rows.slice(0, page.take), hasMore: rows.length > page.take };
});
