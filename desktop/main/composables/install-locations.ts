import { invoke } from "@tauri-apps/api/core";
import type { InstallLocations } from "~/types";

export function fetchInstallLocations() {
  return invoke<InstallLocations>("fetch_install_locations");
}
