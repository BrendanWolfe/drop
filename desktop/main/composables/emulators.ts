import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { MissingEmulator } from "~/types";

export async function useMissingEmulators(gameId: string) {
  const missing = ref<MissingEmulator[]>([]);
  const loading = ref(true);
  const error = ref("");
  let request = 0;
  let disposed = false;
  let unlisten: UnlistenFn | undefined;

  async function refresh() {
    const current = ++request;
    loading.value = true;
    try {
      const result = await invoke<MissingEmulator[]>(
        "fetch_missing_emulators",
        { gameId },
      );
      if (disposed || current !== request) return;
      missing.value = result;
      error.value = "";
    } catch (cause) {
      if (!disposed && current === request)
        error.value = `Could not check required emulators: ${cause}`;
    } finally {
      if (!disposed && current === request) loading.value = false;
    }
  }

  onMounted(async () => {
    window.addEventListener("focus", refresh);
    try {
      const stop = await listen("update_library", refresh);
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
      await refresh();
    } catch (cause) {
      if (!disposed) error.value = `Could not watch emulator changes: ${cause}`;
    }
  });
  onBeforeUnmount(() => {
    window.removeEventListener("focus", refresh);
    disposed = true;
    unlisten?.();
  });
  await refresh();
  return { missing, loading, error, refresh };
}
