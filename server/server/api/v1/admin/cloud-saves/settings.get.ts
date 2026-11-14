import aclManager from "~/server/internal/acls";
import { saveLimits } from "~/server/internal/cloud-saves/storage";

export default defineEventHandler(async (event) => {
  if (!(await aclManager.allowSystemACL(event, ["settings:update"])))
    throw createError({ statusCode: 403 });
  const limits = await saveLimits();
  return {
    sizeLimitMiB: limits.archiveBytes / 1024 / 1024,
    history: limits.history,
  };
});
