<template>
  <section
    class="mt-8 border-t border-zinc-700 py-6"
    aria-labelledby="client-updates-title"
  >
    <header class="flex flex-wrap items-center justify-between gap-4">
      <div>
        <h3
          id="client-updates-title"
          class="font-display text-base font-semibold text-zinc-100"
        >
          Drop client updates
        </h3>
        <p class="mt-1 text-sm text-zinc-400">
          Installed version
          <span class="ml-1 font-mono text-zinc-200">{{
            status.releaseTag || status.version || "Loading…"
          }}</span>
        </p>
      </div>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          class="update-settings-button bg-zinc-800 text-zinc-100 hover:bg-zinc-700"
          :disabled="
            busy || status.phase === 'disabled' || status.phase === 'ready'
          "
          @click="check"
        >
          Check for updates
        </button>
        <button
          v-if="status.phase === 'ready'"
          type="button"
          class="update-settings-button bg-blue-600 text-white hover:bg-blue-500"
          :disabled="pending"
          @click="install"
        >
          {{ updateButtonLabel(status) }}
        </button>
      </div>
    </header>
    <p role="status" class="mt-4 text-sm text-zinc-300">{{ description }}</p>
    <p v-if="status.disabledReason" class="mt-2 text-sm text-zinc-400">
      {{ status.disabledReason }}
    </p>
    <div class="mt-5 flex items-start justify-between gap-4">
      <div>
        <h4
          id="automatic-client-updates"
          class="text-sm font-medium text-zinc-100"
        >
          Automatic updates
        </h4>
        <p
          id="automatic-client-updates-help"
          class="mt-1 max-w-xl text-sm text-zinc-400"
        >
          Check for and download signed client updates in the background.
          Installing an update still asks you to update and restart.
        </p>
      </div>
      <Switch
        :model-value="status.automaticUpdates ?? true"
        :disabled="pending || loading"
        aria-labelledby="automatic-client-updates"
        aria-describedby="automatic-client-updates-help"
        class="relative inline-flex h-6 w-11 shrink-0 rounded-full border-2 border-transparent transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:opacity-50"
        :class="
          (status.automaticUpdates ?? true) ? 'bg-blue-600' : 'bg-zinc-700'
        "
        @update:model-value="setAutomatic"
      >
        <span
          class="pointer-events-none inline-block size-5 rounded-full bg-white shadow transition-transform"
          :class="
            (status.automaticUpdates ?? true)
              ? 'translate-x-5'
              : 'translate-x-0'
          "
        />
      </Switch>
    </div>
    <dl class="mt-5 grid gap-4 text-sm sm:grid-cols-2">
      <div>
        <dt class="text-zinc-400">Last checked</dt>
        <dd class="mt-1 tabular-nums text-zinc-200">
          <time v-if="lastChecked" :datetime="lastChecked.iso">{{
            lastChecked.label
          }}</time
          ><span v-else>Never checked</span>
        </dd>
      </div>
      <div>
        <dt class="text-zinc-400">Last updated</dt>
        <dd class="mt-1 tabular-nums text-zinc-200">
          <time v-if="lastUpdated" :datetime="lastUpdated.iso">{{
            lastUpdated.label
          }}</time
          ><span v-else>Not recorded yet</span>
        </dd>
      </div>
    </dl>
    <p class="mt-2 text-xs text-zinc-500">
      Times use your local timezone. Last updated is recorded when a changed
      client version first starts.
    </p>
    <p
      v-if="error || status.error"
      role="alert"
      class="mt-3 text-sm text-red-300"
    >
      {{ error || status.error }}
    </p>
  </section>
</template>

<script setup lang="ts">
import { Switch } from "@headlessui/vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  updateButtonLabel,
  updateTimestamp,
  type DesktopUpdateStatus,
} from "~/utils/desktop-update";

const status = ref<DesktopUpdateStatus>({
  phase: "idle",
  tag: null,
  downloaded: 0,
  total: null,
  error: null,
});
const loading = ref(true);
const pending = ref(false);
const error = ref("");
const busy = computed(
  () =>
    loading.value ||
    pending.value ||
    ["checking", "downloading", "installing"].includes(status.value.phase),
);
const lastChecked = computed(() => updateTimestamp(status.value.lastChecked));
const lastUpdated = computed(() => updateTimestamp(status.value.lastUpdated));
const description = computed(() => {
  if (status.value.phase === "disabled")
    return "Client updates are unavailable in this installation.";
  if (status.value.phase === "ready")
    return `${status.value.tag} is downloaded and verified. Ready to update and restart.`;
  if (status.value.phase === "idle")
    return status.value.lastChecked
      ? "No client update is currently pending."
      : "No update check has been recorded yet.";
  return updateButtonLabel(status.value);
});
let stop: UnlistenFn | undefined;
let disposed = false;
onMounted(async () => {
  try {
    let receivedEvent = false;
    const unlisten = await listen<DesktopUpdateStatus>(
      "desktop-update",
      (event) => {
        receivedEvent = true;
        status.value = event.payload;
      },
    );
    if (disposed) {
      unlisten();
      return;
    }
    stop = unlisten;
    const snapshot = await invoke<DesktopUpdateStatus>("desktop_update_status");
    if (!disposed && !receivedEvent) status.value = snapshot;
  } catch (cause) {
    if (!disposed)
      error.value = `Could not load client update status: ${cause}`;
  } finally {
    loading.value = false;
  }
});
onBeforeUnmount(() => {
  disposed = true;
  stop?.();
});

async function action(command: string, args?: Record<string, unknown>) {
  if (pending.value) return;
  pending.value = true;
  error.value = "";
  try {
    await invoke(command, args);
  } catch (cause) {
    error.value = String(cause);
  } finally {
    pending.value = false;
  }
}
function check() {
  return action("check_desktop_update");
}
function install() {
  return action("install_desktop_update");
}
async function setAutomatic(enabled: boolean) {
  if (pending.value) return;
  pending.value = true;
  error.value = "";
  try {
    const result = await invoke<DesktopUpdateStatus>(
      "set_desktop_automatic_updates",
      { enabled },
    );
    status.value.automaticUpdates = result.automaticUpdates;
  } catch (cause) {
    error.value = `Could not save automatic updates: ${cause}`;
  } finally {
    pending.value = false;
  }
}
</script>

<style scoped>
.update-settings-button {
  @apply inline-flex min-h-10 items-center justify-center rounded-md px-3 py-2 text-sm font-semibold transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:cursor-not-allowed disabled:opacity-50;
}
</style>
