<template>
  <div class="min-h-full space-y-6 bg-zinc-950 p-4">
    <div
      class="flex min-h-20 flex-wrap overflow-hidden rounded-xl bg-zinc-900 ring-1 ring-inset ring-white/5"
    >
      <div
        class="flex w-36 shrink-0 flex-col justify-center px-4 py-3 font-display tabular-nums"
      >
        <span
          class="font-bold"
          :class="isPaused ? 'text-amber-300' : 'text-zinc-100'"
          >{{
            isPaused ? "Paused" : `${formatKilobytes(displayStats.speed)}B/s`
          }}</span
        >
        <span class="mt-1 text-xs text-zinc-400">{{
          isPaused
            ? "Downloads paused"
            : `${formatTime(displayStats.time)} left`
        }}</span>
      </div>
      <!-- The graph owns its own column; it can never sit behind the controls. -->
      <div
        class="relative min-w-12 flex-1 overflow-hidden bg-zinc-950/60"
        aria-hidden="true"
      >
        <div
          v-if="!isPaused"
          class="absolute inset-0 flex items-end justify-end gap-px px-2 pt-2"
        >
          <div
            v-for="(bar, index) in speedHistory"
            :key="index"
            class="w-[3px] shrink-0 rounded-t bg-blue-500"
            :style="{ height: `${(bar / speedMax) * 100}%` }"
          />
        </div>
      </div>
      <div
        v-if="showDownloadControls"
        class="flex shrink-0 items-center bg-zinc-900 p-3"
      >
        <button
          type="button"
          @click="toggleDownloads"
          :disabled="downloadControlPending"
          :aria-busy="downloadControlPending"
          class="inline-flex min-h-10 items-center gap-2 rounded-md bg-zinc-100 px-4 py-2 text-sm font-semibold text-zinc-950 shadow-sm hover:bg-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:cursor-wait disabled:opacity-50"
        >
          <PauseIcon
            v-if="!isPaused"
            class="size-4"
            aria-hidden="true"
          /><PlayIcon v-else class="size-4" aria-hidden="true" />
          {{ downloadControlLabel }}
        </button>
      </div>
    </div>

    <p
      v-if="downloadControlError"
      role="alert"
      class="rounded-lg bg-red-500/10 p-4 text-sm text-red-200"
    >
      {{ downloadControlError }}
    </p>

    <section aria-labelledby="active-heading">
      <div class="mb-3 flex items-baseline justify-between gap-4">
        <h1
          id="active-heading"
          class="font-display text-lg font-semibold text-zinc-100"
        >
          {{ isPaused ? "Paused download" : "Active download" }}
        </h1>
        <span class="text-xs text-zinc-500"
          >Drop a queued download here to switch</span
        >
      </div>
      <draggable
        v-model="activeItems"
        item-key="key"
        :group="{ name: 'downloads', put: true }"
        handle=".download-handle"
        :sort="false"
        :disabled="queueBusy"
        data-slot="active"
        class="min-h-36 rounded-xl bg-blue-500/5 p-2 ring-1 ring-inset ring-blue-400/20"
        ghost-class="opacity-30"
        @start="onDragStart"
        @end="onDragEnd"
      >
        <template #item="{ element }">
          <DownloadQueueCard
            :item="element"
            :title="title(element.meta)"
            :cover="games[element.meta.id]?.cover"
            :metadata-error="metadataErrors[element.meta.id]"
            active
            :paused="isPaused"
            :disabled="queueBusy"
            @cancel="cancelGame(element.meta)"
          />
        </template>
        <template #footer
          ><p
            v-if="!activeItems.length"
            class="px-4 py-10 text-center text-sm text-zinc-500"
          >
            No active download. Choose a game from your library to download.
          </p></template
        >
      </draggable>
    </section>

    <section
      aria-labelledby="queued-heading"
      class="border-t border-zinc-800 pt-5"
    >
      <div class="mb-3 flex items-baseline justify-between gap-4">
        <h2
          id="queued-heading"
          class="font-display text-sm font-semibold text-zinc-300"
        >
          Up next
          <span class="ml-2 tabular-nums text-zinc-500">{{
            Math.max(0, queue.queue.length - 1)
          }}</span>
        </h2>
        <span class="text-xs text-zinc-500">Drag to reorder</span>
      </div>
      <draggable
        v-model="waitingItems"
        item-key="key"
        group="downloads"
        handle=".download-handle"
        :disabled="queueBusy"
        data-slot="waiting"
        class="min-h-24 space-y-2"
        ghost-class="opacity-30"
        :animation="reducedMotion ? 0 : 150"
        @start="onDragStart"
        @end="onDragEnd"
      >
        <template #item="{ element, index }">
          <DownloadQueueCard
            :item="element"
            :title="title(element.meta)"
            :cover="games[element.meta.id]?.cover"
            :metadata-error="metadataErrors[element.meta.id]"
            :disabled="queueBusy"
            :first="index === 0"
            :last="index === waitingItems.length - 1"
            @cancel="cancelGame(element.meta)"
            @activate="requestMove(element.meta, 0)"
            @move="moveWaiting(element.meta, $event)"
          />
        </template>
        <template #footer
          ><p
            v-if="!waitingItems.length"
            class="rounded-lg border border-dashed border-zinc-800 px-4 py-8 text-center text-sm text-zinc-500"
          >
            No downloads waiting.
          </p></template
        >
      </draggable>
    </section>

    <ModalTemplate
      :model-value="!!pendingSwitch"
      @update:model-value="if (!$event) pendingSwitch = undefined;"
    >
      <template #default>
        <DialogTitle class="font-display text-lg font-semibold text-zinc-100"
          >Switch active download?</DialogTitle
        >
        <p class="mt-3 text-sm leading-6 text-zinc-400">
          Move “{{ pendingSwitch ? title(pendingSwitch.meta) : "" }}”
          {{
            pendingSwitch?.newIndex === 0
              ? "into the active slot"
              : "back into the queue"
          }}? The current download will stop and keep its progress.
          {{
            isPaused
              ? "Downloads will remain paused."
              : "The new active download will start now."
          }}
        </p>
      </template>
      <template #buttons>
        <button
          type="button"
          class="min-h-10 rounded-md bg-blue-600 px-4 py-2 text-sm font-semibold text-white hover:bg-blue-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400"
          @click="confirmSwitch"
        >
          Switch download
        </button>
        <button
          type="button"
          class="min-h-10 rounded-md bg-zinc-800 px-4 py-2 text-sm text-zinc-100 hover:bg-zinc-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400"
          @click="pendingSwitch = undefined"
        >
          Keep current download
        </button>
      </template>
    </ModalTemplate>
  </div>
</template>

<script setup lang="ts">
import { PauseIcon, PlayIcon } from "@heroicons/vue/20/solid";
import { invoke } from "@tauri-apps/api/core";
import { DialogTitle } from "@headlessui/vue";
import type { DownloadableMetadata, Game } from "~/types";
import { downloadKey, queueMove } from "~/utils/download-queue";

const queue = useQueueState();
const stats = useStatsState();
const speedHistory = useDownloadHistory();
const speedMax = computed(() => Math.max(1, ...speedHistory.value) * 1.1);
const reducedMotion = ref(false);
onMounted(() => {
  reducedMotion.value = window.matchMedia(
    "(prefers-reduced-motion: reduce)",
  ).matches;
});
const isPaused = computed(() => queue.value.status === "Paused");
const displayStats = computed(() =>
  isPaused.value ? { speed: 0, time: 0 } : stats.value,
);
const showDownloadControls = computed(() => queue.value.queue.length > 0);
const downloadControlPending = ref(false);
const downloadControlError = ref<string>();
const downloadControlLabel = computed(() =>
  downloadControlPending.value
    ? "Updating…"
    : isPaused.value
      ? "Resume Downloads"
      : "Pause Downloads",
);

async function toggleDownloads() {
  if (downloadControlPending.value) return;
  downloadControlPending.value = true;
  downloadControlError.value = undefined;
  const paused = isPaused.value;
  try {
    await invoke(paused ? "resume_downloads" : "pause_downloads");
    await waitForQueue(
      () =>
        !queue.value.queue.length ||
        (paused
          ? queue.value.status === "Downloading"
          : queue.value.status === "Paused"),
    );
  } catch (error) {
    controlError(error);
  } finally {
    downloadControlPending.value = false;
  }
}

function controlError(error: unknown) {
  downloadControlError.value = String(error);
  void invoke("report_client_error", {
    source: "client",
    message: String(error),
    details: "Download queue control",
  }).catch(() => {});
}

type Item = QueueState["queue"][number] & { key: string };
const activeItems = ref<Item[]>([]);
const waitingItems = ref<Item[]>([]);
const dragging = ref(false);
const saving = ref(false);
const pendingSwitch = ref<{
  meta: DownloadableMetadata;
  newIndex: number;
  front: DownloadableMetadata;
}>();
const queueBusy = computed(() => saving.value || !!pendingSwitch.value);
let dragFront: DownloadableMetadata | undefined;

function syncQueue() {
  const items = queue.value.queue.map((item) => ({
    ...item,
    key: downloadKey(item.meta),
  }));
  activeItems.value = items.slice(0, 1);
  waitingItems.value = items.slice(1);
}
function onDragStart() {
  dragging.value = true;
  dragFront = queue.value.queue[0]?.meta;
}
function onDragEnd(event: {
  item: HTMLElement;
  to: HTMLElement;
  newIndex?: number;
}) {
  dragging.value = false;
  const move = queueMove(
    queue.value.queue,
    event.item.dataset.downloadKey ?? "",
    event.to.dataset.slot ?? "",
    event.newIndex ?? -1,
  );
  syncQueue();
  if (
    !dragFront ||
    downloadKey(queue.value.queue[0]?.meta ?? dragFront) !==
      downloadKey(dragFront)
  ) {
    downloadControlError.value =
      "The active download changed while dragging. Please try again.";
    return;
  }
  if (move) requestMove(move.meta, move.newIndex);
}
function moveWaiting(meta: DownloadableMetadata, direction: number) {
  const index = queue.value.queue.findIndex(
    (item) => downloadKey(item.meta) === downloadKey(meta),
  );
  if (index + direction < 1) return;
  requestMove(meta, index + direction);
}
function requestMove(meta: DownloadableMetadata, newIndex: number) {
  if (queueBusy.value) return;
  const front = queue.value.queue[0]?.meta;
  const oldIndex = queue.value.queue.findIndex(
    (item) => downloadKey(item.meta) === downloadKey(meta),
  );
  if (
    !front ||
    oldIndex < 0 ||
    oldIndex === newIndex ||
    newIndex >= queue.value.queue.length
  )
    return;
  if (oldIndex === 0 || newIndex === 0)
    pendingSwitch.value = { meta, newIndex, front };
  else void saveMove(meta, newIndex, front);
}
async function confirmSwitch() {
  const change = pendingSwitch.value;
  pendingSwitch.value = undefined;
  if (!change) return;
  if (
    downloadKey(queue.value.queue[0]?.meta ?? change.meta) !==
    downloadKey(change.front)
  ) {
    downloadControlError.value =
      "The active download changed. Please choose a download again.";
    return;
  }
  await saveMove(change.meta, change.newIndex, change.front);
}
async function saveMove(
  meta: DownloadableMetadata,
  newIndex: number,
  expectedFront: DownloadableMetadata,
) {
  saving.value = true;
  downloadControlError.value = undefined;
  try {
    await invoke("move_queued_download", { meta, newIndex, expectedFront });
    await waitForQueue(
      () =>
        downloadKey(queue.value.queue[newIndex]?.meta ?? expectedFront) ===
        downloadKey(meta),
    );
  } catch (error) {
    controlError(error);
  } finally {
    saving.value = false;
    syncQueue();
  }
}

// IPC resolves when a command is queued, not when the worker has stopped and
// published the new order. Keep controls locked until authoritative state arrives.
function waitForQueue(ready: () => boolean): Promise<void> {
  if (ready()) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const stop = watch(queue, () => {
      if (!ready()) return;
      clearTimeout(timeout);
      stop();
      resolve();
    });
    const timeout = setTimeout(() => {
      stop();
      reject(new Error("The download queue did not update. Please try again."));
    }, 10_000);
  });
}
async function cancelGame(meta: DownloadableMetadata) {
  try {
    await invoke("cancel_game", { meta });
  } catch (error) {
    controlError(error);
  }
}

const games = ref<Record<string, { game: Game; cover: string }>>({});
const metadataErrors = ref<Record<string, boolean>>({});
const loadingMetadata = new Set<string>();
function title(meta: DownloadableMetadata) {
  return games.value[meta.id]?.game.mName ?? meta.id;
}
function loadGames() {
  for (const {
    meta: { id },
  } of queue.value.queue) {
    if (games.value[id] || loadingMetadata.has(id) || metadataErrors.value[id])
      continue;
    loadingMetadata.add(id);
    void (async () => {
      try {
        const { game } = await useGame(id);
        games.value[id] = { game, cover: await useObject(game.mCoverObjectId) };
      } catch {
        metadataErrors.value[id] = true;
      } finally {
        loadingMetadata.delete(id);
      }
    })();
  }
}
watch(
  queue,
  (value, previous) => {
    if (!dragging.value) syncQueue();
    loadGames();
    if (
      value.queue[0]?.meta.id !== previous?.queue[0]?.meta.id ||
      value.status !== previous?.status
    ) {
      speedHistory.value = [];
      stats.value = { time: 0, speed: 0 };
    }
  },
  { immediate: true },
);
watch(stats, (value) => {
  if (isPaused.value || !value.speed) return;
  speedHistory.value.push(value.speed);
  if (speedHistory.value.length > 240) speedHistory.value.shift();
});
function formatTime(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  return minutes < 60
    ? `${minutes}m ${Math.round(seconds % 60)}s`
    : `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}
</script>
