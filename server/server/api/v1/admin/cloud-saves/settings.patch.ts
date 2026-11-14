import aclManager from "~/server/internal/acls";
import { applicationSettings } from "~/server/internal/config/application-configuration";
import { parseStorageLimits } from "~/server/internal/cloud-saves/validation";

export default defineEventHandler(async (event) => {
  if (!(await aclManager.allowSystemACL(event, ["settings:update"])))
    throw createError({ statusCode: 403 });
  let limits;
  try {
    limits = parseStorageLimits(await readBody(event));
  } catch (error) {
    throw createError({ statusCode: 400, message: (error as Error).message });
  }
  await applicationSettings.set("saveSlotSizeLimit", limits.sizeLimitMiB);
  await applicationSettings.set("saveSlotHistoryLimit", limits.history);
  return limits;
});
