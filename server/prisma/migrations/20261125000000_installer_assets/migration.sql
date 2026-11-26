CREATE TABLE "InstallerAsset" (
  "id" TEXT NOT NULL PRIMARY KEY,
  "versionId" TEXT NOT NULL REFERENCES "GameVersion"("versionId") ON DELETE CASCADE ON UPDATE CASCADE,
  "name" TEXT NOT NULL,
  "sourcePath" TEXT NOT NULL,
  "targetPath" TEXT NOT NULL,
  "manifest" JSONB NOT NULL
);
CREATE INDEX "InstallerAsset_versionId_idx" ON "InstallerAsset"("versionId");
