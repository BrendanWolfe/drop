<template>
  <ModalTemplate v-model="open">
    <DialogTitle class="text-base font-semibold text-zinc-100">
      Uninstall {{ emulator.name }}?
    </DialogTitle>
    <p class="text-sm text-zinc-400">
      This removes version {{ emulator.versionName }} and its installed files
      from this device.
    </p>
    <div
      v-if="emulator.requiredBy.length"
      class="rounded-lg bg-amber-500/10 p-3 text-sm text-amber-200"
    >
      <p>These installed games need this emulator version:</p>
      <ul class="mt-2 list-inside list-disc space-y-1">
        <li v-for="game in emulator.requiredBy" :key="game.gameId">
          {{ game.name }}
          <span class="text-amber-200/70">{{ game.versionName }}</span>
        </li>
      </ul>
      <p class="mt-3">
        You will need to update these games to re-download the emulator before
        playing. Your game files will stay installed.
      </p>
    </div>
    <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
    <template #buttons>
      <button
        type="button"
        class="uninstall-button text-red-300"
        :disabled="uninstalling"
        @click="uninstall"
      >
        {{ uninstalling ? "Uninstalling…" : "Uninstall" }}
      </button>
      <button
        type="button"
        class="uninstall-button text-zinc-100"
        :disabled="uninstalling"
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
import type { InstalledEmulator } from "~/types";

const open = defineModel<boolean>({ required: true });
const props = defineProps<{ emulator: InstalledEmulator }>();
const emit = defineEmits<{ uninstalled: [] }>();
const uninstalling = ref(false);
const error = ref("");

async function uninstall() {
  if (uninstalling.value) return;
  uninstalling.value = true;
  error.value = "";
  try {
    await invoke("uninstall_emulator", {
      gameId: props.emulator.gameId,
      versionId: props.emulator.versionId,
      targetPlatform: props.emulator.platform,
      confirmedDependencies: props.emulator.requiredBy.map(
        (game) => game.gameId,
      ),
    });
    open.value = false;
    emit("uninstalled");
  } catch (cause) {
    error.value = `Could not uninstall the emulator: ${cause}`;
  } finally {
    uninstalling.value = false;
  }
}
</script>

<style scoped>
.uninstall-button {
  @apply inline-flex min-h-10 items-center justify-center rounded-md bg-zinc-800 px-3 py-2 text-sm font-semibold transition-colors hover:bg-zinc-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:cursor-not-allowed disabled:opacity-40;
}
</style>
