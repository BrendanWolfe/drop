<template>
  <main class="mx-auto max-w-5xl space-y-8 px-8 py-10 text-zinc-100">
    <header class="flex items-start gap-6">
      <img :src="useObject('')" alt="" class="size-28 rounded-xl" />
      <div class="min-w-0 flex-1">
        <span
          class="rounded border border-zinc-700 px-2 py-0.5 text-xs font-medium text-zinc-400"
          >Local · {{ game.platform }}</span
        >
        <h1 class="mt-3 break-words font-display text-4xl font-bold">
          {{ game.name }}
        </h1>
        <p class="mt-2 text-sm text-zinc-400">
          {{ (game.playtimeSeconds / 3600).toFixed(1) }} hours played on this
          device
        </p>
      </div>
    </header>

    <div class="flex flex-wrap gap-3">
      <button
        v-if="isRunning"
        class="primary-button"
        :disabled="busy"
        @click="action('kill_game', { gameId: id })"
      >
        <StopIcon class="size-5" />Stop
      </button>
      <button
        v-else-if="ready"
        class="primary-button"
        :disabled="busy"
        @click="launch"
      >
        <PlayIcon class="size-5" />Play
      </button>
      <button
        v-else-if="game.archive?.extracted"
        class="primary-button"
        @click="editOpen = true"
      >
        <FolderOpenIcon class="size-5" />Choose launcher
      </button>
      <button
        v-else-if="!inQueue && game.archive"
        class="primary-button"
        :disabled="busy"
        @click="action('resume_archive', { id })"
      >
        <ArrowDownTrayIcon class="size-5" />{{
          game.archive.downloaded ? "Retry extraction" : "Resume download"
        }}
      </button>
      <NuxtLink v-if="inQueue" to="/queue" class="secondary-button"
        >View download</NuxtLink
      >
      <button
        class="secondary-button"
        :disabled="!canConfigure"
        @click="editOpen = true"
      >
        Edit game
      </button>
      <button
        v-if="ready"
        class="secondary-button"
        :disabled="isRunning"
        @click="compatibilityOpen = true"
      >
        Launch settings
      </button>
      <button
        class="secondary-button"
        @click="action('open_fs', { path: game.root })"
      >
        Open folder
      </button>
      <button
        class="secondary-button"
        :disabled="isRunning || inQueue"
        @click="removeOpen = true"
      >
        Remove
      </button>
    </div>

    <p
      v-if="error"
      role="alert"
      class="rounded-lg border border-red-500/20 bg-red-500/10 p-4 text-sm text-red-300"
    >
      {{ error }}
    </p>

    <section
      v-if="game.archive && !ready"
      class="rounded-xl border border-zinc-700 bg-zinc-950/40 p-6"
    >
      <div class="flex items-center justify-between gap-4">
        <h2 class="font-display text-lg font-semibold">{{ phase }}</h2>
        <span v-if="queueEntry" class="text-sm tabular-nums text-zinc-400"
          >{{ Math.round(progress * 100) }}%</span
        >
      </div>
      <div
        v-if="queueEntry"
        class="mt-4 h-2 overflow-hidden rounded-full bg-zinc-800"
        role="progressbar"
        :aria-valuenow="Math.round(progress * 100)"
        aria-valuemin="0"
        aria-valuemax="100"
        :aria-label="phase"
      >
        <div
          class="h-full bg-blue-500"
          :style="{ width: `${progress * 100}%` }"
        />
      </div>
      <p class="mt-3 text-sm text-zinc-400">
        {{
          game.archive.extracted
            ? "Your files are ready. Choose an executable or script to finish adding this game."
            : "Files extract directly into the game folder. You can resume an interrupted installation from here."
        }}
      </p>
      <p
        v-if="game.archive.error"
        role="alert"
        class="mt-3 text-sm text-red-300"
      >
        {{ game.archive.error }}
      </p>
    </section>

    <section class="rounded-xl border border-zinc-800 bg-zinc-950/30 p-6">
      <h2 class="font-display text-lg font-semibold">On this device</h2>
      <dl class="mt-4 space-y-4 text-sm">
        <div>
          <dt class="text-zinc-500">Game folder</dt>
          <dd class="mt-1 break-all text-zinc-300">{{ game.root }}</dd>
        </div>
        <div>
          <dt class="text-zinc-500">Launch file</dt>
          <dd class="mt-1 break-all text-zinc-300">
            {{ game.launcher || "Not selected" }}
          </dd>
        </div>
        <div>
          <dt class="text-zinc-500">Working directory</dt>
          <dd class="mt-1 break-all text-zinc-300">
            {{ game.workingDirectory }}
          </dd>
        </div>
      </dl>
      <p class="mt-5 border-t border-zinc-800 pt-4 text-xs text-zinc-500">
        {{
          game.archive
            ? "Downloaded by Drop. Automatic updates and server cloud saves are not available."
            : "Your files are managed outside Drop. Removing this entry leaves them untouched."
        }}
      </p>
    </section>

    <ModalTemplate v-if="editOpen" v-model="editOpen" size-class="max-w-2xl">
      <template #default
        ><h2 class="mb-5 font-display text-xl font-semibold text-zinc-100">
          {{ game.launcher ? "Edit game" : "Choose launcher" }}
        </h2>
        <LocalGameForm :game="game" @saved="saved" @cancel="editOpen = false"
      /></template>
      <template #buttons><span /></template>
    </ModalTemplate>
    <GameOptionsModal
      v-if="ready && compatibilityOpen"
      v-model="compatibilityOpen"
      :game-id="id"
      local
    />
    <ModalTemplate v-model="removeOpen">
      <template #default>
        <h2 class="font-display text-xl font-semibold text-zinc-100">
          Remove {{ game.name }}?
        </h2>
        <p class="mt-3 text-sm text-zinc-400">
          {{
            game.archive
              ? "Remove this entry from your library. Keep the game folder unless you choose to delete it below."
              : "This only removes the library entry. Your game files will not be deleted."
          }}
        </p>
        <label
          v-if="game.archive"
          class="mt-5 flex items-start gap-3 text-sm text-zinc-200"
          ><input
            v-model="deleteFiles"
            type="checkbox"
            class="mt-0.5 rounded border-zinc-600 bg-zinc-800 text-red-500 focus:ring-red-500"
          /><span
            >Also permanently delete the installed files in
            <span class="break-all text-zinc-400">{{ game.root }}</span></span
          ></label
        >
      </template>
      <template #buttons
        ><button
          class="rounded-md bg-red-600 px-4 py-2 text-sm font-semibold text-white hover:bg-red-500 disabled:opacity-50"
          :disabled="busy"
          @click="remove"
        >
          {{
            busy
              ? "Removing…"
              : deleteFiles
                ? "Delete files and remove"
                : "Remove from library"
          }}</button
        ><button class="secondary-button" @click="removeOpen = false">
          Cancel
        </button></template
      >
    </ModalTemplate>
  </main>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  PlayIcon,
  StopIcon,
  FolderOpenIcon,
  ArrowDownTrayIcon,
} from "@heroicons/vue/24/solid";
import type { LocalGame } from "~/types";
const route = useRoute();
const router = useRouter();
const id = String(route.params.id);
const game = ref(await invoke<LocalGame>("fetch_local_game", { id }));
const { status } = await useGame(id);
const queue = useQueueState();
const queueEntry = computed(() =>
  queue.value.queue.find((entry) => entry.meta.id === id),
);
const inQueue = computed(() => !!queueEntry.value);
const isRunning = computed(() => status.value.type === "Running");
const ready = computed(
  () =>
    !!game.value.launcher &&
    (!game.value.archive || game.value.archive.extracted),
);
const canConfigure = computed(
  () =>
    !isRunning.value &&
    !inQueue.value &&
    (!game.value.archive || game.value.archive.extracted),
);
const progress = computed(() =>
  Math.max(
    0,
    Math.min(
      1,
      (queueEntry.value?.status === "Extracting"
        ? queueEntry.value.disk_progress
        : queueEntry.value?.dl_progress) ?? 0,
    ),
  ),
);
const phase = computed(() =>
  inQueue.value && queue.value.status === "Paused"
    ? "Paused"
    : (queueEntry.value?.status ??
      (game.value.archive?.extracted
        ? "Choose launcher"
        : game.value.archive?.state === "Error"
          ? "Installation needs attention"
          : "Installation incomplete")),
);
const editOpen = ref(game.value.archive?.state === "NeedsLauncher");
const compatibilityOpen = ref(false);
const removeOpen = ref(false);
const deleteFiles = ref(false);
const busy = ref(false);
const error = ref("");
async function refresh() {
  const previous = game.value;
  game.value = await invoke<LocalGame>("fetch_local_game", { id });
  if (!previous.archive?.extracted && game.value.archive?.extracted)
    editOpen.value = true;
}
const unlisten = await listen<string>("update_local_game", (event) => {
  if (event.payload === id)
    void refresh().catch((e) => {
      error.value = String(e);
    });
});
onUnmounted(unlisten);
watch(isRunning, (running) => {
  if (!running)
    void refresh().catch((e) => {
      error.value = String(e);
    });
});
async function action(command: string, args: Record<string, unknown>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await invoke(command, args);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
async function launch() {
  await action("launch_game", { id, index: 0 });
}
async function saved() {
  editOpen.value = false;
  await refresh();
}
async function remove() {
  busy.value = true;
  error.value = "";
  try {
    await invoke("remove_local_game", { id, deleteFiles: deleteFiles.value });
    await router.push("/library");
  } catch (e) {
    error.value = String(e);
    removeOpen.value = false;
  } finally {
    busy.value = false;
  }
}
</script>

<style scoped>
.primary-button {
  @apply inline-flex items-center gap-2 rounded-md bg-blue-600 px-6 py-3 font-display font-semibold text-white hover:bg-blue-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-50;
}
.secondary-button {
  @apply inline-flex items-center justify-center rounded-md border border-zinc-700 bg-zinc-800/50 px-4 py-2 text-sm font-medium text-zinc-200 hover:bg-zinc-800 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:cursor-not-allowed disabled:opacity-40;
}
</style>
