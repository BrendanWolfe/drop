import type { DownloadableMetadata } from "../types.ts";

export function downloadKey(meta: DownloadableMetadata): string {
  return JSON.stringify([
    meta.id,
    meta.version,
    meta.targetPlatform,
    meta.downloadType,
  ]);
}

export function queueMove<T extends { meta: DownloadableMetadata }>(
  queue: T[],
  key: string,
  slot: string,
  index: number,
) {
  const oldIndex = queue.findIndex((item) => downloadKey(item.meta) === key);
  if (
    oldIndex < 0 ||
    !["active", "waiting"].includes(slot) ||
    !Number.isInteger(index) ||
    index < 0
  )
    return undefined;
  const newIndex =
    slot === "active" ? 0 : Math.min(index + 1, queue.length - 1);
  if (oldIndex === newIndex) return undefined;
  return {
    meta: queue[oldIndex]!.meta,
    newIndex,
    changesActive: oldIndex === 0 || newIndex === 0,
  };
}
