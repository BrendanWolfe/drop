ALTER TABLE "GameVersion" ADD COLUMN "installerBundleId" TEXT;
CREATE UNIQUE INDEX "GameVersion_installerBundleId_key" ON "GameVersion"("installerBundleId");
