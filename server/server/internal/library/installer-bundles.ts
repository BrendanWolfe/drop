import { rm } from "node:fs/promises";
import path from "node:path";
import { systemConfig } from "../config/sys-conf";
import { logger } from "../logging";

export function installerBundleRoot() {
  return path.resolve(systemConfig.getDataFolder(), "installer-bundles");
}

export function installerBundlePath(id: string) {
  if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/u.test(id))
    throw new Error("Invalid installer bundle ID.");
  return path.join(installerBundleRoot(), id);
}

export async function removeInstallerBundles(
  versions: Array<{ installerBundleId: string | null }>,
) {
  for (const { installerBundleId } of versions) {
    if (!installerBundleId) continue;
    try {
      await rm(installerBundlePath(installerBundleId), {
        force: true,
        recursive: true,
      });
    } catch (cause) {
      logger.warn(
        `Could not remove installer bundle ${installerBundleId}: ${cause}`,
      );
    }
  }
}
