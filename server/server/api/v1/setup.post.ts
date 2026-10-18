import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import { completeSetup } from "~/server/internal/setup";

export default defineEventHandler(async (h3) => {
  const allowed = await aclManager.allowSystemACL(h3, ["setup"]);
  if (!allowed)
    throw createError({
      statusCode: 403,
      statusMessage: "Must use a setup token.",
    });

  const completed = await completeSetup(prisma);
  if (!completed)
    throw createError({
      statusCode: 400,
      statusMessage:
        "Cannot finish setup until an enabled admin account with an enabled sign-in method exists.",
    });
});
