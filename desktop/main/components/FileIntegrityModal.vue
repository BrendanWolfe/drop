<template>
  <ModalTemplate v-model="open">
    <DialogTitle class="text-base font-semibold text-zinc-100">
      Verify file integrity for {{ name }}?
    </DialogTitle>
    <p class="text-sm text-zinc-400">
      Drop will check the installed version and download replacements for missing
      or corrupt files. It will not upgrade the game or remove unrelated files.
      Modified game files may be replaced; back up your mods first.
    </p>
    <p class="text-sm text-zinc-400">
      An internet connection is required. Archive installs and some older delta
      versions may need additional downloads to check their contents. You can
      pause or cancel from the download queue.
    </p>
    <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
    <template #buttons>
      <button
        type="button"
        class="integrity-button bg-blue-600 text-white hover:bg-blue-500"
        :disabled="starting"
        @click="verify"
      >
        {{ starting ? "Preparing…" : "Verify and repair" }}
      </button>
      <button
        type="button"
        class="integrity-button bg-zinc-800 text-zinc-100 hover:bg-zinc-700"
        :disabled="starting"
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

const open = defineModel<boolean>({ required: true });
const props = defineProps<{
  gameId: string;
  name: string;
  versionId?: string;
  targetPlatform?: string;
}>();
const starting = ref(false);
const error = ref("");
const router = useRouter();

async function verify() {
  if (starting.value) return;
  starting.value = true;
  error.value = "";
  try {
    await invoke("verify_file_integrity", {
      gameId: props.gameId,
      versionId: props.versionId,
      targetPlatform: props.targetPlatform,
    });
    open.value = false;
    await router.push("/queue");
  } catch (cause) {
    error.value = `Could not start verification: ${cause}`;
  } finally {
    starting.value = false;
  }
}
</script>

<style scoped>
.integrity-button {
  @apply inline-flex min-h-10 items-center justify-center rounded-md px-3 py-2 text-sm font-semibold transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:cursor-not-allowed disabled:opacity-40;
}
</style>
