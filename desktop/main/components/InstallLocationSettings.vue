<template>
  <section
    class="border-b border-zinc-700 py-6"
    :aria-labelledby="`${kind}-locations-title`"
  >
    <div class="flex flex-wrap items-start justify-between gap-4">
      <div>
        <h3
          :id="`${kind}-locations-title`"
          class="font-display text-base font-semibold text-zinc-100"
        >
          {{ title }}
        </h3>
        <p class="mt-1 max-w-xl text-sm text-zinc-400">
          {{
            kind === "game"
              ? "Choose where Drop installs games."
              : "Choose where Drop installs emulators, separately from your games."
          }}
          The default is preselected for new downloads. Existing files stay
          where they are.
        </p>
      </div>
      <button
        type="button"
        class="location-button bg-blue-600 text-white hover:bg-blue-500"
        :disabled="busy"
        @click="openAdd"
      >
        Add location
      </button>
    </div>
    <ul class="mt-3 divide-y divide-zinc-800">
      <li
        v-for="(directory, index) in locations.directories"
        :key="directory"
        class="flex flex-wrap items-center justify-between gap-3 py-4"
      >
        <div class="min-w-0 flex-1">
          <p class="break-all text-sm text-zinc-200">{{ directory }}</p>
          <span
            v-if="index === locations.defaultIndex"
            class="mt-1 inline-flex rounded bg-blue-500/10 px-2 py-1 text-xs font-medium text-blue-200"
            >Default</span
          >
        </div>
        <div class="flex items-center gap-2">
          <button
            v-if="index !== locations.defaultIndex"
            type="button"
            class="location-button bg-zinc-800 text-zinc-200 hover:bg-zinc-700"
            :disabled="busy"
            @click="setDefault(index)"
          >
            Set default
          </button>
          <button
            type="button"
            class="location-button bg-zinc-800 text-zinc-400 hover:bg-zinc-700 hover:text-zinc-100"
            :disabled="busy || locations.directories.length <= 1"
            :aria-label="`Remove ${directory} from ${title.toLowerCase()}`"
            @click="remove(index)"
          >
            <TrashIcon class="size-5" aria-hidden="true" />
          </button>
        </div>
      </li>
    </ul>
    <p class="text-xs text-zinc-500">
      Removing a location only removes it from this list; it does not delete
      installed files.
    </p>
    <p v-if="error && !adding" role="alert" class="mt-3 text-sm text-red-300">
      {{ error }}
    </p>
    <p v-if="notice" role="status" class="mt-3 text-sm text-blue-200">
      {{ notice }}
    </p>

    <ModalTemplate v-model="adding">
      <DialogTitle class="text-base font-semibold text-zinc-100"
        >Add {{ kind }} installation location</DialogTitle
      >
      <button
        type="button"
        class="location-button w-full justify-start bg-zinc-800 text-left text-zinc-100 hover:bg-zinc-700"
        :disabled="busy"
        @click="chooseDirectory"
      >
        {{ currentDirectory || "Choose a folder…" }}
      </button>
      <label class="flex items-start gap-2 text-sm text-zinc-300">
        <input
          v-model="allowNonEmpty"
          type="checkbox"
          class="mt-1 rounded border-zinc-600 bg-zinc-800 text-blue-600 focus:ring-blue-600"
        />
        Allow a non-empty folder
      </label>
      <p class="text-sm text-amber-200">
        Use a folder dedicated to Drop installations. Uninstalling an
        application deletes its installed folder and files.
      </p>
      <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
      <template #buttons>
        <LoadingButton
          :loading="busy"
          :disabled="busy || !currentDirectory"
          @click="add"
          >Add location</LoadingButton
        >
        <button
          type="button"
          class="location-button bg-zinc-800 text-zinc-100 hover:bg-zinc-700"
          :disabled="busy"
          @click="adding = false"
        >
          Cancel
        </button>
      </template>
    </ModalTemplate>
  </section>
</template>

<script setup lang="ts">
import { DialogTitle } from "@headlessui/vue";
import { TrashIcon } from "@heroicons/vue/20/solid";
import { invoke } from "@tauri-apps/api/core";
import type { InstallLocationKind, InstallLocationList } from "~/types";

const props = defineProps<{
  kind: InstallLocationKind;
  locations: InstallLocationList;
  refresh: () => Promise<void>;
}>();
const title = computed(() =>
  props.kind === "game" ? "Game locations" : "Emulator locations",
);
const busy = ref(false);
const adding = ref(false);
const currentDirectory = ref("");
const allowNonEmpty = ref(false);
const error = ref("");
const notice = ref("");

function openAdd() {
  currentDirectory.value = "";
  allowNonEmpty.value = false;
  error.value = "";
  notice.value = "";
  adding.value = true;
}

async function chooseDirectory() {
  try {
    const result = await invoke<string | null>("plugin:dialog|open", {
      options: { directory: true },
    });
    if (result) currentDirectory.value = result;
  } catch (cause) {
    error.value = String(cause);
  }
}

async function change(
  command: string,
  args: Record<string, unknown>,
  message: string,
) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  notice.value = "";
  try {
    await invoke(command, { ...args, kind: props.kind });
    adding.value = false;
    notice.value = message;
    await props.refresh();
  } catch (cause) {
    error.value = String(cause);
  } finally {
    busy.value = false;
  }
}

function add() {
  if (!currentDirectory.value) return;
  return change(
    "add_download_dir",
    { newDir: currentDirectory.value, allowNonEmpty: allowNonEmpty.value },
    "Location added.",
  );
}
function setDefault(index: number) {
  return change(
    "set_default_download_dir",
    { path: props.locations.directories[index] },
    "Default location saved.",
  );
}
function remove(index: number) {
  return change(
    "delete_download_dir",
    { path: props.locations.directories[index] },
    "Location removed. Installed files were not deleted.",
  );
}
</script>

<style scoped>
.location-button {
  @apply inline-flex min-h-10 items-center rounded-md px-3 py-2 text-sm font-semibold transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:cursor-not-allowed disabled:opacity-40;
}
</style>
