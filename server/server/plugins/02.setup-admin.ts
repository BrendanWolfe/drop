import prisma from "~/server/internal/db/database";
import { ensureSetupToken } from "../internal/setup";
import { systemConfig } from "../internal/config/sys-conf";
import { logger } from "../internal/logging";

export default defineNitroPlugin(async (_nitro) => {
  // This setup runs every time the server sets up,
  // but has not been configured
  // so it should be in-place

  const token = await ensureSetupToken(prisma);
  if (!token) return;

  const setupUrl = `${systemConfig.getExternalUrl()}/setup?token=${token.token}`;
  logger.info(`Open ${setupUrl} in a browser to get started with Drop.`);
});
