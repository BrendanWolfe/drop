<template>
  <ModalTemplate v-model="visible" size-class="max-w-xl">
    <div class="space-y-4">
      <div>
        <h3 class="text-base font-semibold text-zinc-100">Locate game</h3>
        <p class="mt-1 text-sm text-zinc-400">
          Choose the folder where the installer put the game, then choose the
          file that starts it.
        </p>
      </div>
      <div>
        <label
          for="installer-game-folder"
          class="mb-1 block text-sm font-medium text-zinc-200"
          >Game folder</label
        >
        <div class="flex gap-2">
          <input
            id="installer-game-folder"
            :value="root"
            readonly
            class="block w-full min-w-0 rounded-md border-0 bg-zinc-800 px-3 py-2 text-sm text-zinc-100"
          />
          <button
            type="button"
            :disabled="scanning || saving"
            class="min-h-10 shrink-0 rounded-md bg-zinc-800 px-3 text-sm text-zinc-200 hover:bg-zinc-700"
            @click="chooseFolder"
          >
            Browse
          </button>
        </div>
      </div>
      <p v-if="scanning" role="status" class="text-sm text-zinc-400">
        Looking for launch files…
      </p>
      <div v-else-if="root">
        <label
          for="installer-launch-file"
          class="mb-1 block text-sm font-medium text-zinc-200"
          >Launch file</label
        >
        <LaunchFilePicker
          input-id="installer-launch-file"
          v-model="launcher"
          :root="root"
          :candidates="candidates"
          readonly
          @error="error = $event"
        />
      </div>
      <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
    </div>
    <template #buttons>
      <LoadingButton
        :loading="saving"
        :disabled="!launcher || scanning"
        @click="save"
        >Save launch file</LoadingButton
      >
      <button
        type="button"
        class="min-h-10 rounded-md bg-zinc-800 px-3 text-sm text-zinc-200 hover:bg-zinc-700"
        @click="visible = false"
      >
        Cancel
      </button>
    </template>
  </ModalTemplate>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { InstallerLocation } from "~/types";

const props = defineProps<{
  gameId: string;
  location?: InstallerLocation | null;
}>();
const emit = defineEmits<{ saved: [location: InstallerLocation] }>();
const visible = defineModel<boolean>({ required: true });
const root = ref(props.location?.root ?? "");
const launcher = ref(props.location?.launcher ?? "");
const candidates = ref<string[]>([]);
const error = ref("");
const scanning = ref(false);
const saving = ref(false);

async function chooseFolder() {
  error.value = "";
  try {
    const selected = await open({
      multiple: false,
      directory: true,
      defaultPath: root.value || undefined,
    });
    if (typeof selected !== "string") return;
    scanning.value = true;
    const found = await invoke<string[]>("find_installer_launchers", {
      id: props.gameId,
      root: selected,
    });
    root.value = selected;
    launcher.value = "";
    candidates.value = found;
  } catch (cause) {
    error.value = String(cause);
  } finally {
    scanning.value = false;
  }
}

async function save() {
  saving.value = true;
  error.value = "";
  try {
    const location = await invoke<InstallerLocation>(
      "save_installer_location",
      { id: props.gameId, root: root.value, launcher: launcher.value },
    );
    emit("saved", location);
    visible.value = false;
  } catch (cause) {
    error.value = String(cause);
  } finally {
    saving.value = false;
  }
}
</script>
