import { ArkErrors, type } from "arktype";
import type { Prisma } from "~/prisma/client/client";
import { GameType } from "~/prisma/client/enums";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import gameSizeManager from "~/server/internal/gamesize";
import { parsePlatform } from "~/server/internal/utils/parseplatform";
import { getAgeRestrictionFilter } from "~/server/internal/utils/ageRestrictions";

const StoreRead = type({
  skip: type("string")
    .pipe((s) => Number.parseInt(s))
    .default("0"),
  take: type("string")
    .pipe((s) => Number.parseInt(s))
    .default("10"),

  tags: "string?",
  platform: "string?",

  company: "string?",
  companyActions: "string = 'published,developed'",

  sort: "'default' | 'newest' | 'recent' | 'name' | 'size' = 'default'",
  order: "'asc' | 'desc' = 'desc'",
});

export default defineEventHandler(async (h3) => {
  const user = await aclManager.getUserACL(h3, ["store:read"]);
  if (!user) throw createError({ statusCode: 403 });
  const userId = user.id;

  const query = getQuery(h3);
  const options = StoreRead(query);
  if (options instanceof ArkErrors)
    throw createError({ statusCode: 400, statusMessage: options.summary });

  const filterPlatforms = options.platform
    ?.split(",")
    .map(parsePlatform)
    .filter((e) => e !== undefined);

  /**
   * Generic filters
   */
  const tagFilter = options.tags
    ? {
        tags: {
          some: {
            id: {
              in: options.tags.split(","),
            },
          },
        },
      }
    : undefined;
  const platformFilter = filterPlatforms
    ? ({
        OR: [
          {
            versions: {
              some: {
                setups: {
                  some: {
                    platform: {
                      in: filterPlatforms,
                    },
                  },
                },
              },
            },
          },
          {
            versions: {
              some: {
                launches: {
                  some: {
                    platform: {
                      in: filterPlatforms,
                    },
                  },
                },
              },
            },
          },
        ],
      } satisfies Prisma.GameWhereInput)
    : undefined;

  /**
   * Company filtering
   */
  const companyActions = options.companyActions.split(",");
  const developedFilter = companyActions.includes("developed")
    ? {
        developers: {
          some: {
            id: options.company!,
          },
        },
      }
    : undefined;
  const publishedFilter = companyActions.includes("published")
    ? {
        publishers: {
          some: {
            id: options.company!,
          },
        },
      }
    : undefined;
  const companyFilter = options.company
    ? ({
        OR: [developedFilter, publishedFilter].filter((e) => e !== undefined),
      } satisfies Prisma.GameWhereInput)
    : undefined;

  /**
   * Query
   */

  const ageFilter = await getAgeRestrictionFilter(userId, user.admin);

  const finalFilter: Prisma.GameWhereInput = {
    ...tagFilter,
    ...platformFilter,
    ...companyFilter,
    ...ageFilter,
    type: GameType.Game,
  };

  if (options.sort === "size") {
    const games = await prisma.game.findMany({
      where: finalFilter,
      select: {
        id: true,
        versions: {
          orderBy: { versionIndex: "desc" },
          take: 1,
          select: { versionId: true },
        },
      },
    });
    // Size is computed from manifests, so sort all matches before paginating.
    const sizedGames = await Promise.all(
      games.map(async (game) => {
        const version = game.versions[0];
        const size = version
          ? await gameSizeManager.getVersionSize(version.versionId)
          : null;
        return { id: game.id, size: size?.installSize ?? null };
      }),
    );
    sizedGames.sort((a, b) => {
      if (a.size === null && b.size !== null) return 1;
      if (b.size === null && a.size !== null) return -1;
      const difference = (a.size ?? 0) - (b.size ?? 0);
      return (
        (options.order === "asc" ? difference : -difference) ||
        a.id.localeCompare(b.id)
      );
    });
    const ids = sizedGames
      .slice(options.skip, options.skip + Math.min(options.take, 50))
      .map((game) => game.id);
    const results = await prisma.game.findMany({
      where: { ...finalFilter, id: { in: ids } },
    });
    results.sort((a, b) => ids.indexOf(a.id) - ids.indexOf(b.id));
    return { results, count: games.length };
  }

  const sort: Prisma.GameOrderByWithRelationInput = {};
  switch (options.sort) {
    case "default":
    case "newest":
      sort.mReleased = options.order;
      break;
    case "recent":
      sort.created = options.order;
      break;
    case "name":
      sort.mName = options.order;
      break;
  }

  const [results, count] = await prisma.$transaction([
    prisma.game.findMany({
      skip: options.skip,
      take: Math.min(options.take, 50),
      where: finalFilter,
      orderBy: sort,
    }),
    prisma.game.count({ where: finalFilter }),
  ]);

  return { results, count };
});
