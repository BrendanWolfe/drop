CREATE TABLE "DownloadHistory" (
    "id" TEXT NOT NULL,
    "clientId" TEXT NOT NULL,
    "sessionId" TEXT NOT NULL,
    "userId" TEXT NOT NULL,
    "userName" TEXT NOT NULL,
    "deviceName" TEXT NOT NULL,
    "gameId" TEXT NOT NULL,
    "gameTitle" TEXT NOT NULL,
    "versionId" TEXT NOT NULL,
    "platform" TEXT NOT NULL,
    "state" TEXT NOT NULL,
    "queuedAt" TIMESTAMP(3) NOT NULL,
    "startedAt" TIMESTAMP(3),
    "endedAt" TIMESTAMP(3),
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "bytesDownloaded" BIGINT NOT NULL DEFAULT 0,
    "bytesTotal" BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT "DownloadHistory_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "DownloadHistory_clientId_sessionId_key" ON "DownloadHistory"("clientId", "sessionId");
CREATE INDEX "DownloadHistory_queuedAt_id_idx" ON "DownloadHistory"("queuedAt", "id");
CREATE INDEX "DownloadHistory_userId_queuedAt_idx" ON "DownloadHistory"("userId", "queuedAt");

CREATE TABLE "ClientErrorReport" (
    "id" TEXT NOT NULL,
    "clientId" TEXT NOT NULL,
    "reportId" TEXT NOT NULL,
    "userId" TEXT NOT NULL,
    "userName" TEXT NOT NULL,
    "deviceName" TEXT NOT NULL,
    "source" TEXT NOT NULL,
    "message" TEXT NOT NULL,
    "details" TEXT NOT NULL,
    "gameId" TEXT,
    "gameTitle" TEXT,
    "clientVersion" TEXT NOT NULL,
    "occurredAt" TIMESTAMP(3) NOT NULL,
    "receivedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "ClientErrorReport_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "ClientErrorReport_clientId_reportId_key" ON "ClientErrorReport"("clientId", "reportId");
CREATE INDEX "ClientErrorReport_receivedAt_id_idx" ON "ClientErrorReport"("receivedAt", "id");
