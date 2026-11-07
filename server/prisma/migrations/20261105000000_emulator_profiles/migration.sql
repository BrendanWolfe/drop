ALTER TABLE "Game" ADD COLUMN "isEmulated" BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE "LaunchConfiguration" ADD COLUMN "emulatorProfile" JSONB;

UPDATE "Game" SET "isEmulated" = true
WHERE "type" = 'Game' AND "id" IN (
  SELECT gv."gameId" FROM "GameVersion" gv
  JOIN "LaunchConfiguration" lc ON lc."versionId" = gv."versionId"
  WHERE lc."emulatorId" IS NOT NULL
);
