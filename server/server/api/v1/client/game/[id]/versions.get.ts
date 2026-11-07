import { ArkErrors, type } from "arktype";
import type { Platform } from "~/prisma/client/enums";
import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import prisma from "~/server/internal/db/database";
import type { GameVersionSize } from "~/server/internal/gamesize";
import gameSizeManager from "~/server/internal/gamesize";
import {
  allowsProton,
  downloadLaunches,
} from "~/server/internal/library/platform-policy";

type VersionDownloadOption = {
  gameId: string;
  versionId: string;
  versionIndex: number;
  displayName?: string | undefined;
  versionPath?: string | undefined;
  platform: Platform;
  protonCompatible: boolean;
  protonRequiredContent: VersionDownloadOption["requiredContent"];
  size: GameVersionSize;
  requiredContent: Array<{
    gameId: string;
    versionId: string;
    launchIds: string[];
    versionName: string;
    name: string;
    iconObjectId: string;
    shortDescription: string;
    size: GameVersionSize;
  }>;
};

const Query = type({
  previous: "string?",
});

export default defineClientEventHandler(async (h3) => {
  const id = getRouterParam(h3, "id")!;
  if (!id)
    throw createError({
      statusCode: 400,
      statusMessage: "No ID in router params",
    });

  const query = Query(getQuery(h3));
  if (query instanceof ArkErrors)
    throw createError({ statusCode: 400, message: query.summary });

  const rawVersions = await prisma.gameVersion.findMany({
    where: {
      gameId: id,
    },
    orderBy: {
      versionIndex: "desc", // Latest one first
    },
    select: {
      versionId: true,
      versionIndex: true,
      displayName: true,
      versionPath: true,
      gameId: true,
      onlySetup: true,
      launches: {
        select: {
          platform: true,
          protonCompatible: true,
          emulator: {
            select: {
              launchId: true,
              platform: true,
              protonCompatible: true,
              gameVersion: {
                select: {
                  game: {
                    select: {
                      mName: true,
                      mShortDescription: true,
                      mIconObjectId: true,
                      id: true,
                    },
                  },
                  versionId: true,
                  displayName: true,
                  versionPath: true,
                },
              },
            },
          },
        },
      },
      setups: true,
    },
  });

  const versions: Array<VersionDownloadOption> = (
    await Promise.all(
      rawVersions.map(async (v) => {
        const platformOptions: Map<
          Platform,
          Pick<
            VersionDownloadOption,
            "requiredContent" | "protonRequiredContent" | "protonCompatible"
          >
        > = new Map();

        for (const launch of downloadLaunches(v)) {
          if (!platformOptions.has(launch.platform))
            platformOptions.set(launch.platform, {
              requiredContent: [],
              protonRequiredContent: [],
              protonCompatible: false,
            });

          const option = platformOptions.get(launch.platform)!;
          const protonAllowed = allowsProton(launch);
          option.protonCompatible ||= protonAllowed;

          if ("emulator" in launch && launch.emulator) {
            const gv = launch.emulator.gameVersion;
            const content = {
              gameId: gv.game.id,
              versionId: gv.versionId,
              launchIds: [launch.emulator.launchId],
              versionName: gv.displayName || gv.versionPath || gv.versionId,
              name: gv.game.mName,
              iconObjectId: gv.game.mIconObjectId,
              shortDescription: gv.game.mShortDescription,
              size: (await gameSizeManager.getVersionSize(gv.versionId))!,
            };
            for (const list of [
              option.requiredContent,
              ...(protonAllowed ? [option.protonRequiredContent] : []),
            ]) {
              const existing = list.find(
                (item) => item.versionId === gv.versionId,
              );
              if (!existing)
                list.push({ ...content, launchIds: [...content.launchIds] });
              else if (!existing.launchIds.includes(launch.emulator.launchId))
                existing.launchIds.push(launch.emulator.launchId);
            }
          }
        }

        const size = await gameSizeManager.getVersionSize(
          v.versionId,
          query.previous,
        );

        return platformOptions
          .entries()
          .map(
            ([platform, compatibility]) =>
              ({
                gameId: v.gameId,
                versionId: v.versionId,
                versionIndex: v.versionIndex,
                displayName: v.displayName || undefined,
                versionPath: v.versionPath || undefined,
                platform,
                ...compatibility,
                size: size!,
              }) satisfies VersionDownloadOption,
          )
          .toArray();
      }),
    )
  ).flat();

  return versions;
});
