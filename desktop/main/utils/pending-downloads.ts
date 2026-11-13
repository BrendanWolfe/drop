import type { QueueState } from "../composables/downloads.ts";
import type { DownloadableMetadata } from "../types.ts";
import { downloadKey } from "./download-queue.ts";

export type PendingDownload = {
  meta: DownloadableMetadata;
  name: string;
  coverObjectId?: string | null;
  action: "resume" | "update";
};

export function pendingDownloadItems(
  pending: PendingDownload[],
  queue: Array<{ meta: DownloadableMetadata }>,
) {
  const queuedIds = new Set(queue.map((item) => item.meta.id));
  return pending
    .filter((item) => !queuedIds.has(item.meta.id))
    .map(
      (candidate) =>
        ({
          ...candidate,
          key: downloadKey(candidate.meta),
          status:
            candidate.action === "resume"
              ? "Partially installed"
              : "Update available",
          dl_progress: null,
          dl_current: 0,
          dl_max: 0,
          disk_progress: null,
          disk_current: 0,
          disk_max: 0,
        }) satisfies QueueState["queue"][number] &
          PendingDownload & { key: string },
    );
}

export function pendingDropIndex(
  slot: string,
  index: number,
  queueLength: number,
) {
  if (!Number.isInteger(index) || index < 0) return undefined;
  if (slot === "active") return 0;
  if (slot === "waiting") return Math.min(index + 1, queueLength);
  return undefined;
}

type PendingDownloadControls = {
  readQueue: () => Array<{ meta: DownloadableMetadata }>;
  enqueue: (meta: DownloadableMetadata) => Promise<DownloadableMetadata[]>;
  wait: (ready: () => boolean) => Promise<void>;
  move: (
    meta: DownloadableMetadata,
    index: number,
    front: DownloadableMetadata,
  ) => Promise<void>;
};

export async function addPendingDownload(
  candidate: PendingDownload,
  newIndex: number | undefined,
  controls: PendingDownloadControls,
) {
  const { readQueue, enqueue, wait, move } = controls;
  const originalFront = readQueue()[0]?.meta;
  const batch = await enqueue(candidate.meta);
  const game = batch.at(-1);
  if (!game)
    throw new Error("No download was added. Refresh the list and try again.");
  await wait(() =>
    readQueue().some((item) => downloadKey(item.meta) === downloadKey(game)),
  );
  if (newIndex === undefined) return;

  const front = readQueue()[0]?.meta;
  const sameFront =
    front && originalFront && downloadKey(front) === downloadKey(originalFront);
  const batchIsActive =
    front && batch.some((meta) => downloadKey(meta) === downloadKey(front));
  if (!sameFront && !batchIsActive) {
    throw new Error(
      "Added to the queue, but the active download changed. Choose its position again.",
    );
  }
  // Dependencies that already finished do not need to be moved.
  const queued = batch.filter((meta) =>
    readQueue().some((item) => downloadKey(item.meta) === downloadKey(meta)),
  );
  let insertionIndex = newIndex;
  for (const meta of queued) {
    const index = readQueue().findIndex(
      (item) => downloadKey(item.meta) === downloadKey(meta),
    );
    // An already queued dependency ahead of this slot must stay put, especially
    // when it is the active download and the user only requested Up next.
    if (index < insertionIndex) continue;
    const target = Math.min(insertionIndex++, readQueue().length - 1);
    if (index === target) continue;
    const expectedFront = readQueue()[0]?.meta;
    if (!expectedFront || index < 0) continue;
    await move(meta, target, expectedFront);
    await wait(() => {
      const item = readQueue()[target];
      return !!item && downloadKey(item.meta) === downloadKey(meta);
    });
  }
}
