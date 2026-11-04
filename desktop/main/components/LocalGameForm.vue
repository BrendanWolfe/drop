<template>
  <form class="space-y-4" @submit.prevent="save">
    <div>
      <label for="local-name" class="field-label">Game name</label>
      <input
        id="local-name"
        v-model="name"
        required
        maxlength="200"
        class="field-input"
      />
    </div>
    <div>
      <label for="local-launcher" class="field-label">Launch file</label>
      <div class="flex gap-2">
        <input
          id="local-launcher"
          v-model="launcher"
          required
          class="field-input"
          placeholder="Executable or script"
        />
        <button type="button" class="browse-button" @click="chooseLauncher">
          Browse
        </button>
      </div>
      <select
        v-if="candidates.length"
        class="field-input mt-2"
        aria-label="Detected launch files"
        @change="selectCandidate"
      >
        <option value="">Choose a detected launch file…</option>
        <option v-for="path in candidates" :key="path" :value="path">
          {{ path.slice((game?.root.length ?? 0) + 1) }}
        </option>
      </select>
    </div>
    <div>
      <label for="local-root" class="field-label">Game folder</label>
      <div class="flex gap-2">
        <input
          id="local-root"
          v-model="root"
          required
          :readonly="!!game?.archive"
          class="field-input"
        />
        <button
          v-if="!game?.archive"
          type="button"
          class="browse-button"
          @click="chooseDirectory('root')"
        >
          Browse
        </button>
      </div>
      <p class="mt-1 text-xs text-zinc-400">
        {{
          game?.archive
            ? "Drop manages this downloaded folder."
            : "Your files stay where they are. Removing this game will not delete them."
        }}
      </p>
    </div>
    <div>
      <label for="local-working-directory" class="field-label"
        >Working directory</label
      >
      <div class="flex gap-2">
        <input
          id="local-working-directory"
          v-model="workingDirectory"
          required
          class="field-input"
        />
        <button
          type="button"
          class="browse-button"
          @click="chooseDirectory('working')"
        >
          Browse
        </button>
      </div>
    </div>
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-[1fr,10rem]">
      <div>
        <label for="local-arguments" class="field-label"
          >Launch arguments <span class="text-zinc-500">(optional)</span></label
        >
        <input
          id="local-arguments"
          v-model="argumentsText"
          class="field-input"
          placeholder="--fullscreen"
        />
      </div>
      <div>
        <label for="local-platform" class="field-label">Game platform</label>
        <select
          id="local-platform"
          v-model="targetPlatform"
          class="field-input"
        >
          <option>Windows</option>
          <option>Linux</option>
          <option>macOS</option>
        </select>
      </div>
    </div>
    <p
      v-if="error"
      role="alert"
      class="rounded-md bg-red-500/10 p-3 text-sm text-red-300"
    >
      {{ error }}
    </p>
    <div class="flex justify-end gap-2 pt-2">
      <button type="button" class="browse-button" @click="$emit('cancel')">
        Cancel
      </button>
      <button
        type="submit"
        :disabled="saving"
        class="rounded-md bg-blue-600 px-4 py-2 text-sm font-semibold text-white hover:bg-blue-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-50"
      >
        {{ saving ? "Saving…" : game ? "Save game" : "Add game" }}
      </button>
    </div>
  </form>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { platform } from "@tauri-apps/plugin-os";
import type { LocalGame } from "~/types";

const props = defineProps<{ game?: LocalGame }>();
const emit = defineEmits<{ saved: [id: string]; cancel: [] }>();
const name = ref(props.game?.name ?? "");
const launcher = ref(props.game?.launcher ?? "");
const root = ref(props.game?.root ?? "");
const workingDirectory = ref(props.game?.workingDirectory ?? "");
const argumentsText = ref(props.game?.arguments ?? "");
const host = platform();
const targetPlatform = ref<LocalGame["platform"]>(
  props.game?.platform ??
    (host === "windows" ? "Windows" : host === "macos" ? "macOS" : "Linux"),
);
const saving = ref(false);
const error = ref("");
const candidates = ref<string[]>([]);
onMounted(async () => {
  if (props.game?.archive?.extracted) {
    try {
      candidates.value = await invoke("find_local_launchers", {
        id: props.game.id,
      });
    } catch (e) {
      error.value = String(e);
    }
  }
});
function setLauncher(path: string) {
  launcher.value = path;
  const parent = path.replace(/[\\/][^\\/]+$/, "");
  if (!root.value) root.value = parent;
  workingDirectory.value = parent;
  if (/\.(exe|com|bat|cmd|ps1)$/i.test(path)) targetPlatform.value = "Windows";
  if (!name.value)
    name.value =
      path
        .split(/[\\/]/)
        .pop()
        ?.replace(/\.[^.]+$/, "") ?? "";
}
function selectCandidate(event: Event) {
  const path = (event.target as HTMLSelectElement).value;
  if (path) setLauncher(path);
}
async function chooseLauncher() {
  try {
    const path = await open({
      multiple: false,
      directory: false,
      defaultPath: root.value || undefined,
    });
    if (typeof path === "string") setLauncher(path);
  } catch (e) {
    error.value = String(e);
  }
}
async function chooseDirectory(field: "root" | "working") {
  try {
    const path = await open({ multiple: false, directory: true });
    if (typeof path === "string") {
      if (field === "root") root.value = path;
      else workingDirectory.value = path;
    }
  } catch (e) {
    error.value = String(e);
  }
}
async function save() {
  saving.value = true;
  error.value = "";
  try {
    const id = await invoke<string>("save_local_game", {
      input: {
        id: props.game?.id ?? null,
        name: name.value,
        launcher: launcher.value,
        root: root.value,
        workingDirectory: workingDirectory.value,
        arguments: argumentsText.value,
        platform: targetPlatform.value,
      },
    });
    emit("saved", id);
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}
</script>

<style scoped>
.field-label {
  @apply mb-1.5 block text-sm font-medium text-zinc-200;
}
.field-input {
  @apply block w-full min-w-0 rounded-md border-0 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 ring-1 ring-inset ring-zinc-700 focus:ring-2 focus:ring-blue-500;
}
.browse-button {
  @apply shrink-0 rounded-md bg-zinc-800 px-3 py-2 text-sm font-medium text-zinc-200 ring-1 ring-inset ring-zinc-700 hover:bg-zinc-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400;
}
</style>
