export const STORE_LIBRARY_MESSAGE = "drop:store:library-changed";

export function getStoreLibraryChange(
  event: { source: unknown; data: unknown },
  storeWindow: unknown,
): { gameId: string; download: boolean } | undefined {
  // Custom-protocol origins vary by platform; only trust the Store iframe itself.
  if (!storeWindow || event.source !== storeWindow) return;
  if (!event.data || typeof event.data !== "object") return;

  const message = event.data as Record<string, unknown>;
  if (
    message.type === STORE_LIBRARY_MESSAGE &&
    typeof message.download === "boolean" &&
    typeof message.gameId === "string" &&
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
      message.gameId,
    )
  ) {
    return { gameId: message.gameId, download: message.download };
  }
}
