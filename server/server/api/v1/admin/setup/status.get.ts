import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import { hasEnabledSetupAdmin } from "~/server/internal/setup";

export default defineEventHandler(async (h3) => {
  const allowed = await aclManager.allowSystemACL(h3, ["setup"]);
  if (!allowed)
    throw createError({
      statusCode: 403,
      statusMessage: "Must use a setup token.",
    });

  return {
    hasAdmin: await hasEnabledSetupAdmin(prisma),
  };
});
