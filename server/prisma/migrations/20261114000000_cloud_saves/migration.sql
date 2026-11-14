ALTER TABLE "Game" ADD COLUMN "cloudSavesEnabled" BOOLEAN NOT NULL DEFAULT false,
  ADD COLUMN "cloudSaveRevision" INTEGER NOT NULL DEFAULT 0;

CREATE TABLE "CloudSaveConfig" (
  "gameId" TEXT NOT NULL PRIMARY KEY REFERENCES "Game"("id") ON DELETE CASCADE ON UPDATE CASCADE,
  "locations" JSONB NOT NULL DEFAULT '[]'
);

CREATE TABLE "CloudSaveHead" (
  "id" TEXT NOT NULL PRIMARY KEY,
  "gameId" TEXT NOT NULL REFERENCES "Game"("id") ON DELETE CASCADE ON UPDATE CASCADE,
  "userId" TEXT NOT NULL REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE,
  "namespace" TEXT NOT NULL,
  "revision" INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX "CloudSaveHead_gameId_userId_namespace_key" ON "CloudSaveHead"("gameId", "userId", "namespace");

CREATE TABLE "CloudSaveSnapshot" (
  "id" TEXT NOT NULL PRIMARY KEY,
  "headId" TEXT NOT NULL REFERENCES "CloudSaveHead"("id") ON DELETE CASCADE ON UPDATE CASCADE,
  "revision" INTEGER NOT NULL,
  "definitionRevision" INTEGER NOT NULL,
  "clientId" TEXT NOT NULL,
  "kind" TEXT NOT NULL,
  "objectId" TEXT NOT NULL,
  "checksum" TEXT NOT NULL,
  "contentHash" TEXT NOT NULL,
  "bytes" INTEGER NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX "CloudSaveSnapshot_objectId_key" ON "CloudSaveSnapshot"("objectId");
CREATE UNIQUE INDEX "CloudSaveSnapshot_headId_revision_key" ON "CloudSaveSnapshot"("headId", "revision");
CREATE TABLE "CloudSaveGarbage" (
  "objectId" TEXT NOT NULL PRIMARY KEY,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX "CloudSaveGarbage_createdAt_idx" ON "CloudSaveGarbage"("createdAt");

-- Preserve cleanup work even when the owning user/game is deleted by cascade.
CREATE FUNCTION cloud_save_enqueue_garbage() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO "CloudSaveGarbage" ("objectId") VALUES (OLD."objectId") ON CONFLICT DO NOTHING;
  RETURN OLD;
END;
$$;
CREATE TRIGGER cloud_save_snapshot_deleted AFTER DELETE ON "CloudSaveSnapshot"
  FOR EACH ROW EXECUTE FUNCTION cloud_save_enqueue_garbage();
