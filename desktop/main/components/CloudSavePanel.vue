<template>
  <section
    class="rounded-xl bg-zinc-800/50 p-6"
    aria-labelledby="game-cloud-saves-title"
  >
    <div class="flex flex-wrap items-center justify-between gap-3">
      <h2
        id="game-cloud-saves-title"
        class="font-display text-xl font-semibold text-zinc-100"
      >
        Cloud saves
      </h2>
      <button
        type="button"
        class="cloud-button bg-zinc-700/50 hover:bg-zinc-700"
        :disabled="Boolean(busy) || loading"
        @click="refresh"
      >
        Check cloud saves
      </button>
    </div>
    <p role="status" class="mt-3 text-sm text-zinc-300">
      {{ loading ? "Checking cloud saves…" : status?.message }}
    </p>
    <p v-if="error" role="alert" class="mt-3 text-sm text-red-300">
      {{ error }}
    </p>
    <NuxtLink
      v-if="status && !status.enabled"
      to="/settings"
      class="mt-3 inline-flex min-h-10 items-center text-sm font-semibold text-blue-400 hover:text-blue-300"
      >Open global settings</NuxtLink
    >
    <template v-if="status?.enabled">
      <p class="mt-3 text-sm text-zinc-400">
        Close the game and its launcher before replacing saves. Drop cannot
        reliably detect games started through external launchers.
      </p>
      <label
        class="mt-3 flex min-h-10 items-center gap-3 text-sm text-zinc-200"
      >
        <input
          v-model="confirmedClosed"
          :disabled="Boolean(busy) || running"
          type="checkbox"
          class="rounded border-zinc-600 bg-zinc-900 text-blue-600 focus:ring-blue-500"
        />
        I have closed the game and its launcher
      </label>
      <p v-if="running" class="mt-1 text-sm text-amber-300">
        Drop is tracking an active game process. Close it before syncing
        manually.
      </p>
      <div class="mt-3 flex flex-wrap gap-3">
        <button
          type="button"
          class="cloud-button bg-blue-600 hover:bg-blue-500"
          :disabled="
            !canAct || status.interruptedRestore || status.phase === 'offline'
          "
          @click="upload"
        >
          {{ conflict ? "Use local saves" : "Upload local saves" }}
        </button>
        <button
          v-if="status.interruptedRestore"
          type="button"
          class="cloud-button bg-amber-800 hover:bg-amber-700"
          :disabled="!canAct"
          @click="recover"
        >
          Recover previous local backup
        </button>
        <button
          v-if="status.interruptedRestore"
          type="button"
          class="cloud-button bg-zinc-700 hover:bg-zinc-600"
          :disabled="!canAct"
          @click="retryRestore"
        >
          Retry interrupted restore
        </button>
      </div>
      <div
        v-if="status.snapshots.length && !status.interruptedRestore"
        class="mt-5 space-y-3"
      >
        <label class="block text-sm font-medium text-zinc-200"
          >Cloud snapshot
          <select
            v-model="selected"
            :disabled="Boolean(busy)"
            class="mt-2 min-h-10 w-full rounded-md border-zinc-700 bg-zinc-900 text-sm text-zinc-100 focus:border-blue-500 focus:ring-blue-500"
          >
            <option
              v-for="snapshot in status.snapshots"
              :key="snapshot.id"
              :value="snapshot.id"
            >
              {{ snapshotLabel(snapshot) }}
            </option>
          </select>
        </label>
        <button
          type="button"
          class="cloud-button bg-zinc-700 hover:bg-zinc-600"
          :disabled="!canAct || !selected || status.phase === 'offline'"
          @click="restore"
        >
          Restore cloud snapshot
        </button>
        <p class="text-xs text-zinc-400">
          Restoring replaces the configured save set, including deleted slots. A
          local recovery copy is kept first. Background snapshots may be
          incomplete if captured during a game save. Review the current save
          locations before restoring a snapshot from an older configuration.
        </p>
      </div>
      <details
        v-if="status.locations.length"
        class="mt-5 text-sm text-zinc-400"
      >
        <summary class="cursor-pointer py-2">
          Configured save locations ({{ status.namespace }})
        </summary>
        <ul class="mt-2 space-y-1">
          <li
            v-for="location in status.locations"
            :key="location"
            class="break-all font-mono text-xs"
          >
            {{ location }}
          </li>
        </ul>
      </details>
      <details
        v-if="status.recoveries.length"
        class="mt-3 text-sm text-zinc-400"
      >
        <summary class="cursor-pointer py-2">Local recovery backups</summary>
        <label class="mt-2 block"
          >Backup archive
          <select
            v-model="localBackup"
            class="mt-2 min-h-10 w-full rounded-md border-zinc-700 bg-zinc-900 text-sm text-zinc-100"
            :disabled="Boolean(busy)"
          >
            <option value="" disabled>Select a local backup</option>
            <option v-for="name in status.recoveries" :key="name" :value="name">
              {{ name }}
            </option>
          </select>
        </label>
        <button
          type="button"
          class="cloud-button mt-3 bg-zinc-700 hover:bg-zinc-600"
          :disabled="
            !confirmedClosed ||
            running ||
            Boolean(busy) ||
            !localBackup ||
            status.interruptedRestore
          "
          @click="restoreLocal"
        >
          Restore local backup
        </button>
      </details>
      <button
        v-if="status.recoveryDirectory"
        type="button"
        class="cloud-button mt-3 text-zinc-300 hover:bg-zinc-700"
        :disabled="Boolean(busy)"
        @click="openRecovery"
      >
        Open local recovery folder
      </button>
    </template>
    <p v-if="busy" role="status" class="mt-3 text-sm text-blue-300">
      {{ busy }}… Keep Drop open until this finishes.
    </p>
  </section>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  cloudActionNeedsChoice,
  snapshotLabel,
  type CloudSaveStatus,
} from "~/utils/cloud-saves";

const props = defineProps<{ gameId: string; running: boolean }>();
const status = ref<CloudSaveStatus | null>(null);
const loading = ref(false);
const busy = ref("");
const error = ref("");
const confirmedClosed = ref(false);
const selected = ref("");
const localBackup = ref("");
const conflict = computed(() => cloudActionNeedsChoice(status.value));
const canAct = computed(
  () =>
    confirmedClosed.value && !props.running && !busy.value && !loading.value,
);
let unlisten: UnlistenFn | undefined;
let disposed = false;
async function refresh() {
  if (loading.value || busy.value) return;
  loading.value = true;
  error.value = "";
  const id = props.gameId;
  try {
    const result = await invoke<CloudSaveStatus>("fetch_cloud_save_status", {
      id,
    });
    if (disposed || id !== props.gameId) return;
    status.value = result;
    if (!result.snapshots.some((s) => s.id === selected.value))
      selected.value = result.snapshots[0]?.id ?? "";
  } catch (cause) {
    if (id === props.gameId) {
      status.value = null;
      error.value = String(cause);
    }
  } finally {
    loading.value = false;
  }
}
async function action(label: string, command: string, extra = {}) {
  if (!canAct.value) return;
  busy.value = label;
  error.value = "";
  try {
    await invoke(command, {
      id: props.gameId,
      confirmedClosed: confirmedClosed.value,
      ...extra,
    });
  } catch (cause) {
    error.value = String(cause);
  } finally {
    busy.value = "";
    confirmedClosed.value = false;
  }
  if (!error.value) await refresh();
}
function upload() {
  return action("Uploading snapshot", "upload_cloud_save", {
    useLocal: conflict.value,
    expectedRevision: status.value?.revision,
    definitionRevision: status.value?.definitionRevision,
  });
}
function restore() {
  return action("Restoring snapshot", "restore_cloud_save", {
    snapshotId: selected.value,
  });
}
function recover() {
  return action("Recovering local backup", "recover_cloud_save");
}
function retryRestore() {
  return action("Retrying interrupted restore", "retry_cloud_save_restore");
}
function restoreLocal() {
  return action("Restoring local backup", "restore_local_save_backup", {
    name: localBackup.value,
  });
}
async function openRecovery() {
  try {
    await invoke("open_cloud_save_recovery", { id: props.gameId });
  } catch (cause) {
    error.value = String(cause);
  }
}
watch(
  () => props.running,
  () => {
    confirmedClosed.value = false;
  },
);
watch(
  () => props.gameId,
  () => {
    status.value = null;
    confirmedClosed.value = false;
    selected.value = "";
    void refresh();
  },
);
onMounted(async () => {
  unlisten = await listen<string>("cloud-saves-changed", (event) => {
    if (event.payload === props.gameId) void refresh();
  });
  if (disposed) {
    unlisten();
    return;
  }
  await refresh();
});
onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});
</script>

<style scoped>
.cloud-button {
  @apply inline-flex min-h-10 items-center justify-center rounded-md px-3 py-2 text-sm font-semibold text-zinc-100 transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:cursor-not-allowed disabled:opacity-50;
}
</style>
