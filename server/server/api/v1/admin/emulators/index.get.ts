import { type, ArkErrors } from "arktype";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";

const Query = type({ query: "string?", skip: "string.numeric.parse?" });

export default defineEventHandler(async (h3) => {
  if (!(await aclManager.allowSystemACL(h3, ["library:read"])))
    throw createError({ statusCode: 403 });
  const query = Query(getQuery(h3));
  if (query instanceof ArkErrors)
    throw createError({ statusCode: 400, message: query.summary });
  const where = {
    type: "Emulator" as const,
    ...(query.query
      ? { mName: { contains: query.query, mode: "insensitive" as const } }
      : {}),
  };
  const [results, count] = await Promise.all([
    prisma.game.findMany({
      where,
      skip: Math.max(0, query.skip ?? 0),
      take: 24,
      orderBy: [{ mName: "asc" }, { id: "asc" }],
      select: {
        id: true,
        mName: true,
        mIconObjectId: true,
        versions: {
          orderBy: { versionIndex: "desc" },
          select: {
            versionId: true,
            displayName: true,
            versionPath: true,
            launches: { select: { platform: true } },
          },
        },
      },
    }),
    prisma.game.count({ where }),
  ]);
  return { results, count };
});
