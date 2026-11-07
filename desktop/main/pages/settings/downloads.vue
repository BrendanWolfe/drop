<template>
  <div class="border-b border-zinc-700 py-5">
    <h3 class="text-base font-semibold font-display leading-6 text-zinc-100">
      Downloads
    </h3>
  </div>
  <InstallLocationSettings
    kind="game"
    :locations="locations.game"
    :refresh="refreshLocations"
  />
  <InstallLocationSettings
    kind="emulator"
    :locations="locations.emulator"
    :refresh="refreshLocations"
  />
  <p v-if="locationError" role="alert" class="mt-3 text-sm text-red-300">
    {{ locationError }}
  </p>

  <section class="py-6">
    <h3 class="text-base font-display font-semibold text-zinc-100">
      Download settings
    </h3>
    <p class="mt-1 text-sm text-zinc-400 max-w-xl">
      Configure how Drop downloads games and other content.
    </p>
    <div class="mt-6 max-w-xl">
      <label for="threads" class="block text-sm font-medium text-zinc-100"
        >Maximum download threads</label
      >
      <input
        id="threads"
        v-model="downloadThreads"
        type="number"
        min="1"
        max="32"
        name="threads"
        class="download-input mt-2"
        @keypress="validateNumberInput"
        @paste="validatePaste"
      />
      <p class="mt-2 text-sm text-zinc-400">
        The maximum number of concurrent download threads. Higher values may
        download faster but use more system resources. Default is 4.
      </p>
    </div>
    <div class="mt-6 max-w-xl">
      <label
        for="max-download-speed"
        class="block text-sm font-medium text-zinc-100"
        >Download speed limit</label
      >
      <div class="relative mt-2">
        <input
          id="max-download-speed"
          v-model="maxDownloadSpeedMegabytes"
          type="number"
          name="max-download-speed"
          min="0"
          step="any"
          :aria-invalid="speedLimitError !== undefined"
          aria-describedby="max-download-speed-help"
          class="download-input pr-16"
          @input="speedLimitError = undefined"
        />
        <span
          class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-sm text-zinc-400"
          >MB/s</span
        >
      </div>
      <p id="max-download-speed-help" class="mt-2 text-sm text-zinc-400">
        Maximum download speed in megabytes per second (1 MB/s = 1,000,000 bytes
        per second). Use 0 for unlimited. This applies to active and future
        downloads.
      </p>
      <p v-if="speedLimitError" class="mt-2 text-sm text-red-400" role="alert">
        {{ speedLimitError }}
      </p>
    </div>
    <div class="mt-10 flex items-center justify-between gap-4">
      <div>
        <h3
          id="force-offline-label"
          class="text-sm font-medium leading-6 text-zinc-100"
        >
          Force offline
        </h3>
        <p class="mt-1 text-sm leading-6 text-zinc-400">
          Drop will not make any external connections.
        </p>
      </div>
      <Switch
        v-model="forceOffline"
        aria-labelledby="force-offline-label"
        :class="[
          forceOffline ? 'bg-blue-600' : 'bg-zinc-700',
          'relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400',
        ]"
      >
        <span
          :class="[
            forceOffline ? 'translate-x-5' : 'translate-x-0',
            'pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition-transform duration-200 ease-in-out',
          ]"
        />
      </Switch>
    </div>
    <div class="mt-6">
      <button
        type="button"
        :disabled="saveState.loading"
        :class="[
          'inline-flex min-h-10 items-center rounded-md px-3 py-2 text-sm font-semibold text-white shadow-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 transition-colors',
          saveState.success
            ? 'bg-green-600 hover:bg-green-500 focus-visible:outline-green-600'
            : 'bg-blue-600 hover:bg-blue-500 focus-visible:outline-blue-600',
          'disabled:bg-blue-600/50 disabled:cursor-not-allowed',
        ]"
        @click="saveSettings"
      >
        {{ saveState.success ? "Saved" : "Save changes" }}
      </button>
      <div
        v-if="saveError"
        role="alert"
        class="mt-3 rounded-md bg-red-500/15 p-4 outline outline-red-500/25"
      >
        <h3 class="text-sm font-medium text-red-200">
          Couldn't save download settings
        </h3>
        <p class="mt-1 text-sm text-red-200/80">{{ saveError }}</p>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { Switch } from "@headlessui/vue";
import { invoke } from "@tauri-apps/api/core";
import type { Settings } from "~/types";
import {
  formatDownloadSpeedMegabytes,
  parseDownloadSpeedMegabytes,
} from "~/utils/download-settings";

const locations = ref(await fetchInstallLocations());
const locationError = ref("");
async function refreshLocations() {
  try {
    locations.value = await fetchInstallLocations();
    locationError.value = "";
  } catch (cause) {
    locationError.value = `Could not refresh installation locations: ${cause}. Reopen this page to try again.`;
  }
}

const settings = await invoke<Settings>("fetch_settings");
const downloadThreads = ref(settings?.maxDownloadThreads ?? 4);
const maxDownloadSpeedMegabytes = ref<string | number>(
  formatDownloadSpeedMegabytes(settings?.maxDownloadSpeed ?? 0),
);
const speedLimitError = ref<string | undefined>();
const saveError = ref<string | undefined>();
const forceOffline = ref(settings?.forceOffline ?? false);
const saveState = reactive({ loading: false, success: false });
let savedTimer: ReturnType<typeof setTimeout> | undefined;
onBeforeUnmount(() => clearTimeout(savedTimer));

async function saveSettings() {
  saveError.value = undefined;
  const speedLimit = parseDownloadSpeedMegabytes(
    maxDownloadSpeedMegabytes.value,
  );
  if (!speedLimit.valid) {
    speedLimitError.value = speedLimit.error;
    return;
  }
  speedLimitError.value = undefined;
  try {
    saveState.loading = true;
    await invoke("update_settings", {
      newSettings: {
        maxDownloadThreads: downloadThreads.value,
        maxDownloadSpeed: speedLimit.bytesPerSecond,
        forceOffline: forceOffline.value,
      },
    });
    saveState.success = true;
    clearTimeout(savedTimer);
    savedTimer = setTimeout(() => {
      saveState.success = false;
    }, 2000);
  } catch (cause) {
    saveError.value = String(cause);
  } finally {
    saveState.loading = false;
  }
}

function validateNumberInput(event: KeyboardEvent) {
  if (
    !/^\d$/.test(event.key) &&
    !["Backspace", "Delete", "Tab", "ArrowLeft", "ArrowRight"].includes(
      event.key,
    )
  )
    event.preventDefault();
}
function validatePaste(event: ClipboardEvent) {
  const pasted = event.clipboardData?.getData("text");
  if (pasted && !/^\d+$/.test(pasted)) event.preventDefault();
}
</script>

<style scoped>
.download-input {
  @apply block min-h-10 w-full rounded-md border-0 bg-zinc-800 py-1.5 text-zinc-100 shadow-sm ring-1 ring-inset ring-zinc-700 focus:ring-2 focus:ring-inset focus:ring-blue-600 sm:text-sm sm:leading-6;
}
</style>
