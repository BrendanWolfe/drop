<template>
  <div>
    <div class="flex gap-2">
      <input
        :id="inputId"
        v-model="launcher"
        :readonly="readonly"
        required
        class="field-input"
        placeholder="Executable or script"
      />
      <button type="button" class="browse-button" @click="browse">
        Browse
      </button>
    </div>
    <select
      v-if="candidates.length"
      class="field-input mt-2"
      aria-label="Detected launch files"
      :value="candidates.includes(launcher) ? launcher : ''"
      @change="selectCandidate"
    >
      <option value="">Choose a detected launch file…</option>
      <option v-for="path in candidates" :key="path" :value="path">
        {{
          root && path.startsWith(root + separator)
            ? path.slice(root.length + 1)
            : path
        }}
      </option>
    </select>
  </div>
</template>

<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";

const props = defineProps<{
  inputId: string;
  root: string;
  candidates: string[];
  readonly?: boolean;
}>();
const launcher = defineModel<string>({ required: true });
const emit = defineEmits<{
  selected: [path: string];
  error: [error: string];
}>();
const separator = computed(() => (props.root.includes("\\") ? "\\" : "/"));

function select(path: string) {
  if (!path) return;
  launcher.value = path;
  emit("selected", path);
}

function selectCandidate(event: Event) {
  select((event.target as HTMLSelectElement).value);
}

async function browse() {
  try {
    const path = await open({
      multiple: false,
      directory: false,
      defaultPath: props.root || undefined,
    });
    if (typeof path === "string") select(path);
  } catch (cause) {
    emit("error", String(cause));
  }
}
</script>

<style scoped>
.field-input {
  @apply block w-full min-w-0 rounded-md border-0 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 ring-1 ring-inset ring-zinc-700 focus:ring-2 focus:ring-blue-500;
}
.browse-button {
  @apply min-h-10 shrink-0 rounded-md bg-zinc-800 px-3 py-2 text-sm font-medium text-zinc-200 ring-1 ring-inset ring-zinc-700 hover:bg-zinc-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400;
}
</style>
