<template>
  <section
    class="mt-8 border-t border-zinc-700 py-6"
    aria-labelledby="cloud-save-settings-title"
  >
    <div class="flex items-start justify-between gap-6">
      <div>
        <h3
          id="cloud-save-settings-title"
          class="font-display text-base font-semibold text-zinc-100"
        >
          Cloud saves
        </h3>
        <p
          id="cloud-save-settings-help"
          class="mt-2 max-w-xl text-sm text-zinc-400"
        >
          Enable cloud saves for all supported games on this device. Only games
          enabled by your server administrator participate. Saves are private to
          your account on your Drop server.
        </p>
      </div>
      <Switch
        :model-value="enabled"
        :disabled="loading || saving"
        aria-labelledby="cloud-save-settings-title"
        aria-describedby="cloud-save-settings-help"
        class="relative inline-flex min-h-10 w-11 shrink-0 items-center rounded-md focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:opacity-50"
        @update:model-value="setEnabled"
      >
        <span
          class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors"
          :class="enabled ? 'bg-blue-600' : 'bg-zinc-700'"
        >
          <span
            class="inline-block size-5 rounded-full bg-white shadow transition-transform"
            :class="enabled ? 'translate-x-[22px]' : 'translate-x-0.5'"
          />
        </span>
      </Switch>
    </div>
    <p class="mt-4 max-w-2xl text-sm text-zinc-400">
      While Drop is running, changed save folders are checked every minute.
      Stable changes are uploaded as recovery snapshots, even for games started
      through Steam. A quiet folder is not proof that the game has closed. Close
      the game before using Upload local or Restore cloud on its library page.
      Restores are never automatic.
    </p>
    <p class="mt-2 text-sm text-zinc-400">
      Turning this off stops synchronization without deleting local or server
      backups.
    </p>
    <p v-if="error" role="alert" class="mt-3 text-sm text-red-300">
      {{ error }}
    </p>
  </section>
</template>

<script setup lang="ts">
import { Switch } from "@headlessui/vue";
import { invoke } from "@tauri-apps/api/core";

const enabled = ref(false);
const loading = ref(true);
const saving = ref(false);
const error = ref("");
onMounted(async () => {
  try {
    enabled.value =
      (await invoke<{ cloudSavesEnabled: boolean }>("fetch_settings"))
        .cloudSavesEnabled ?? false;
  } catch (cause) {
    error.value = String(cause);
  } finally {
    loading.value = false;
  }
});
async function setEnabled(value: boolean) {
  saving.value = true;
  error.value = "";
  try {
    await invoke("set_cloud_saves_enabled", { enabled: value });
    enabled.value = value;
  } catch (cause) {
    error.value = String(cause);
  } finally {
    saving.value = false;
  }
}
</script>
