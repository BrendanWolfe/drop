import aclManager from "~/server/internal/acls";
import { downloadRegistry } from "~/server/internal/downloads/registry";

export default defineEventHandler(async (h3) => {
  if (!(await aclManager.allowSystemACL(h3, ["downloads:read"]))) {
    throw createError({ statusCode: 403 });
  }
  return downloadRegistry.snapshot();
});
