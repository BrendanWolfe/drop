ALTER TABLE "GameVersion" ADD COLUMN "locateAfterSetup" BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE "InstallerAsset" ADD COLUMN "filesystemKind" TEXT, ADD COLUMN "filesystemKey" TEXT, ADD COLUMN "filesystemEntry" TEXT;
ALTER TABLE "SetupConfiguration" ADD COLUMN "emulatorId" TEXT;
ALTER TABLE "SetupConfiguration" ADD CONSTRAINT "SetupConfiguration_emulatorId_fkey" FOREIGN KEY ("emulatorId") REFERENCES "LaunchConfiguration"("launchId") ON DELETE RESTRICT ON UPDATE CASCADE;
CREATE INDEX "InstallerAsset_filesystemKey_idx" ON "InstallerAsset"("filesystemKey");
