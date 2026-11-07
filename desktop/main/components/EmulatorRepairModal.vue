<template>
  <ModalTemplate v-model="open">
    <DialogTitle class="text-base font-semibold text-zinc-100"
      >Update required emulators</DialogTitle
    >
    <p class="text-sm text-zinc-400">
      Drop will download these exact emulator versions. Your installed game
      version will not change.
    </p>
    <ul class="space-y-2 text-sm text-zinc-200">
      <li
        v-for="emulator in requirements"
        :key="`${emulator.gameId}:${emulator.versionId}`"
      >
        {{ emulator.name }}
        <span class="ml-2 font-mono text-blue-200">{{
          emulator.versionName
        }}</span>
        <span class="ml-2 text-zinc-400">{{ emulator.platform }}</span>
      </li>
    </ul>
    <InstallDirectorySelector
      v-if="installDirs.length"
      :install-dirs="installDirs"
      v-model="installDir"
      label="Emulator location"
      :default-index="defaultIndex"
    />
    <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
    <p v-if="loading" role="status" class="text-sm text-zinc-400">
      Adding emulator downloads to the queue…
    </p>
    <template #buttons>
      <LoadingButton
        :loading="loading"
        :disabled="loading || !installDirs.length"
        @click="repair"
        >Update</LoadingButton
      >
      <button
        type="button"
        :disabled="loading"
        class="min-h-10 rounded-md bg-zinc-800 px-3 py-2 text-sm font-semibold text-zinc-100 hover:bg-zinc-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-40"
        @click="open = false"
      >
        Cancel
      </button>
    </template>
  </ModalTemplate>
</template>

<script setup lang="ts">
import { DialogTitle } from "@headlessui/vue";
import { invoke } from "@tauri-apps/api/core";
import type { MissingEmulator } from "~/types";
import { emulatorRepairPlan } from "~/utils/emulator-repair";
import { defaultInstallIndex } from "~/utils/install-locations";

const open = defineModel<boolean>({ required: true });
const props = defineProps<{ requirements: MissingEmulator[] }>();
const emit = defineEmits<{ queued: [] }>();
const installDirs = ref<string[]>([]);
const installDir = ref(0);
const defaultIndex = ref<number | null>(null);
const loading = ref(false);
const error = ref("");

async function repair() {
  if (loading.value || !installDirs.value.length) return;
  loading.value = true;
  error.value = "";
  try {
    for (const dependency of emulatorRepairPlan(props.requirements)) {
      await invoke("download_game", {
        gameId: dependency.gameId,
        versionId: dependency.versionId,
        targetPlatform: dependency.platform,
        installPath: installDirs.value[installDir.value],
        enableUpdates: false,
      });
    }
    open.value = false;
    emit("queued");
  } catch (cause) {
    error.value = `Could not download the required emulator: ${cause}`;
  } finally {
    loading.value = false;
  }
}

onMounted(async () => {
  try {
    const locations = (await fetchInstallLocations()).emulator;
    installDirs.value = locations.directories;
    defaultIndex.value = locations.defaultIndex;
    installDir.value = defaultInstallIndex(locations);
    if (!installDirs.value.length)
      error.value =
        "Add an installation folder in App settings, then try Update again.";
    if (open.value && installDirs.value.length === 1) await repair();
  } catch (cause) {
    error.value = `Could not load installation folders: ${cause}`;
  }
});
</script>
