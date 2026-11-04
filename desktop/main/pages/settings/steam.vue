<template>
  <section class="max-w-3xl space-y-5 py-5 text-zinc-100">
    <h1 class="font-display text-xl font-semibold">Steam shortcuts</h1>
    <p class="text-sm text-zinc-400">
      Manage game shortcuts from each game's Steam options. Deferred cleanup for
      uninstalled games and other Drop servers appears below. Exit Steam
      completely before removing shortcuts.
    </p>
    <p v-if="!windows" class="text-sm text-zinc-400">
      Steam integration is available on Windows only.
    </p>
    <template v-else>
      <button
        class="rounded-md bg-zinc-800 px-4 py-2 text-sm hover:bg-zinc-700"
        @click="refresh"
      >
        Refresh
      </button>
      <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
      <p v-if="!entries.length" class="text-sm text-zinc-400">
        No deferred shortcuts to clean up.
      </p>
      <article
        v-for="entry in entries"
        :key="entry.key"
        class="flex items-center justify-between gap-4 rounded-lg border border-zinc-800 p-4"
      >
        <div class="min-w-0">
          <h2 class="truncate font-medium">{{ entry.name }}</h2>
          <p class="text-xs text-zinc-500">Steam profile {{ entry.profile }}</p>
        </div>
        <button
          :disabled="busy"
          class="shrink-0 rounded-md bg-zinc-800 px-3 py-2 text-sm text-red-300 hover:bg-zinc-700 disabled:opacity-50"
          @click="remove(entry.key)"
        >
          Remove from Steam
        </button>
      </article>
    </template>
  </section>
</template>
<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { platform } from "@tauri-apps/plugin-os";
const windows = platform() === "windows";
const entries = ref<Array<{ key: string; name: string; profile: string }>>([]);
const error = ref("");
const busy = ref(false);
async function refresh() {
  if (!windows) return;
  try {
    entries.value = await invoke("steam_cleanup");
  } catch (e) {
    error.value = String(e);
  }
}
async function remove(key: string) {
  busy.value = true;
  error.value = "";
  try {
    await invoke("steam_remove", { key });
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
onMounted(refresh);
</script>
