<template>
  <main class="mx-auto max-w-5xl space-y-6 px-6 py-8 text-zinc-100">
    <header class="flex flex-wrap items-start justify-between gap-4">
      <div>
        <h1 class="font-display text-2xl font-semibold text-zinc-100">
          Emulators
        </h1>
        <p class="mt-2 text-sm text-zinc-400">
          Manage emulator versions installed on this device and see which games
          need them.
        </p>
      </div>
      <button
        type="button"
        class="emulator-button"
        :disabled="loading"
        @click="refresh"
      >
        Refresh
      </button>
    </header>

    <div
      v-if="error"
      role="alert"
      class="rounded-lg bg-red-950/40 p-4 text-sm text-red-300"
    >
      {{ error }}
    </div>
    <p
      v-if="loading && !emulators.length"
      role="status"
      class="text-sm text-zinc-400"
    >
      Loading installed emulators…
    </p>
    <div
      v-else-if="!error && !emulators.length"
      class="rounded-xl bg-zinc-950/50 p-8 text-center"
    >
      <h2 class="font-display text-lg font-semibold text-zinc-100">
        No emulators installed
      </h2>
      <p class="mt-2 text-sm text-zinc-400">
        Install an emulated game from the store. Drop downloads its required
        emulator version automatically.
      </p>
      <NuxtLink to="/store" class="emulator-button mt-5">Browse store</NuxtLink>
    </div>

    <ul class="space-y-4" aria-label="Installed emulators">
      <li
        v-for="emulator in emulators"
        :key="`${emulator.gameId}:${emulator.versionId}`"
        class="rounded-xl bg-zinc-950/50 p-5 shadow-sm ring-1 ring-white/5"
      >
        <div class="flex flex-wrap items-start justify-between gap-4">
          <div class="min-w-0">
            <h2 class="font-display text-lg font-semibold text-zinc-100">
              {{ emulator.name }}
            </h2>
            <p class="mt-1 flex flex-wrap items-center gap-2 text-sm">
              <span
                class="rounded bg-blue-500/10 px-2 py-1 font-mono text-blue-200"
                >{{ emulator.versionName }}</span
              >
              <span class="text-zinc-400">{{ emulator.platform }}</span>
              <span v-if="emulator.busy" role="status" class="text-amber-200"
                >Operation in progress</span
              >
              <span
                v-else-if="emulator.installType.type === 'PartiallyInstalled'"
                class="text-amber-200"
                >Installation incomplete</span
              >
            </p>
          </div>
          <div class="flex flex-wrap gap-2">
            <NuxtLink
              :to="`/library/${emulator.gameId}`"
              class="emulator-button"
              >Manage version</NuxtLink
            >
            <button
              type="button"
              class="emulator-button"
              @click="openFolder(emulator)"
            >
              Open folder
            </button>
            <button
              type="button"
              class="emulator-button text-red-300 hover:enabled:text-red-200"
              :disabled="emulator.busy"
              :aria-describedby="
                emulator.requiredBy.length
                  ? `required-${emulator.gameId}`
                  : undefined
              "
              @click="
                selected = emulator;
                confirmOpen = true;
              "
            >
              Uninstall
            </button>
          </div>
        </div>
        <p class="mt-3 break-all text-xs text-zinc-500">
          {{ emulator.installDir }}
        </p>
        <div class="mt-5">
          <h3
            :id="`required-${emulator.gameId}`"
            class="text-sm font-medium text-zinc-200"
          >
            Required by installed games
            <span class="ml-1 tabular-nums text-zinc-500"
              >({{ emulator.requiredBy.length }})</span
            >
          </h3>
          <ul v-if="emulator.requiredBy.length" class="mt-2 space-y-2">
            <li
              v-for="game in emulator.requiredBy"
              :key="game.gameId"
              class="text-sm"
            >
              <NuxtLink
                :to="`/library/${game.gameId}`"
                class="text-blue-300 hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400"
              >
                {{ game.name }}
              </NuxtLink>
              <span class="ml-2 text-zinc-500">{{ game.versionName }}</span>
            </li>
          </ul>
          <p v-else class="mt-2 text-sm text-zinc-400">
            No installed games require this version.
          </p>
          <p
            v-if="emulator.requiredBy.length"
            class="mt-3 text-xs text-zinc-400"
          >
            Uninstalling this version will require these games to download it
            again before playing.
          </p>
        </div>
      </li>
    </ul>
    <p v-if="emulators.length" class="text-xs text-zinc-500">
      Drop keeps one active version per emulator. Games that require another
      version will ask you to switch when launched.
    </p>

    <EmulatorUninstallModal
      v-if="selected && confirmOpen"
      v-model="confirmOpen"
      :emulator="selected"
      @uninstalled="refresh"
    />
  </main>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { InstalledEmulator } from "~/types";

useHead({ title: "Emulators" });
const emulators = ref<InstalledEmulator[]>([]);
const loading = ref(false);
const error = ref("");
const selected = ref<InstalledEmulator>();
const confirmOpen = ref(false);

async function refresh() {
  loading.value = true;
  error.value = "";
  try {
    emulators.value = await invoke<InstalledEmulator[]>(
      "fetch_installed_emulators",
    );
  } catch (cause) {
    error.value = `Could not load installed emulators: ${cause}. Try refreshing.`;
  } finally {
    loading.value = false;
  }
}

async function openFolder(emulator: InstalledEmulator) {
  try {
    await invoke("open_fs", { path: emulator.installDir });
  } catch (cause) {
    error.value = `Could not open the emulator folder: ${cause}`;
  }
}

let unlisten: UnlistenFn | undefined;
let disposed = false;
onMounted(async () => {
  let watchError = "";
  try {
    const stop = await listen("update_library", refresh);
    if (disposed) {
      stop();
      return;
    }
    unlisten = stop;
  } catch (cause) {
    watchError = `Could not watch emulator changes: ${cause}. Use Refresh to update this list.`;
  }
  if (!disposed) {
    await refresh();
    if (watchError && !error.value) error.value = watchError;
  }
});
onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});
</script>

<style scoped>
.emulator-button {
  @apply inline-flex min-h-10 items-center justify-center rounded-md bg-zinc-800 px-3 py-2 text-sm font-semibold transition-colors hover:bg-zinc-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-zinc-800;
}
</style>
