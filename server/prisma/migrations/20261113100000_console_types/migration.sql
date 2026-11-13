CREATE TABLE "ConsoleType" (
    "id" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "nameKey" TEXT NOT NULL,
    CONSTRAINT "ConsoleType_pkey" PRIMARY KEY ("id")
);

CREATE UNIQUE INDEX "ConsoleType_nameKey_key" ON "ConsoleType"("nameKey");
ALTER TABLE "Game" ADD COLUMN "consoleTypeId" TEXT;
CREATE INDEX "Game_consoleTypeId_idx" ON "Game"("consoleTypeId");
ALTER TABLE "Game" ADD CONSTRAINT "Game_consoleTypeId_fkey"
    FOREIGN KEY ("consoleTypeId") REFERENCES "ConsoleType"("id")
    ON DELETE SET NULL ON UPDATE CASCADE;
