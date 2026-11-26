<template>
  <details
    v-if="supportFiles.length"
    class="mt-4 max-w-2xl rounded-xl bg-zinc-900/90 p-4 text-sm text-zinc-300 ring-1 ring-white/10"
  >
    <summary class="cursor-pointer font-semibold text-zinc-100">
      Installer files
    </summary>
    <p class="mt-2 text-zinc-400">
      These support files were downloaded with the installer.
    </p>
    <ul class="mt-2 list-inside list-disc space-y-1">
      <li
        v-for="file in supportFiles"
        :key="file.name"
        class="flex items-center justify-between gap-2"
      >
        <span class="break-all">{{ file.name }}</span>
        <button
          v-if="file.path"
          type="button"
          class="min-h-10 shrink-0 px-2 text-blue-300 hover:underline"
          :aria-label="`Copy path for ${file.name}`"
          @click="copyPath({ ...file, key: `installer:${file.name}` }, $event)"
        >
          {{ copiedFile === `installer:${file.name}` ? "Copied" : "Copy path" }}
        </button>
      </li>
    </ul>
  </details>
  <p v-if="copyError" role="alert" class="mt-2 text-sm text-amber-200">
    {{ copyError }}
  </p>
  <section
    v-if="rows.length || loadError || inputError"
    aria-labelledby="installer-files-title"
    class="mt-4 max-w-2xl space-y-3 rounded-xl bg-zinc-900/90 p-4 ring-1 ring-white/10"
  >
    <div>
      <h2
        id="installer-files-title"
        class="text-sm font-semibold text-zinc-100"
      >
        Companion files
      </h2>
      <p class="mt-1 text-sm text-zinc-400">
        If the installer asks for a file or folder, copy its path and paste it
        into the installer’s picker.
      </p>
    </div>
    <p v-if="loadError" role="alert" class="text-sm text-red-300">
      {{ loadError }}
    </p>
    <p v-if="inputError" role="alert" class="text-sm text-red-300">
      Could not check the installed library games.
      <button
        type="button"
        class="min-h-10 px-2 text-blue-300 underline"
        @click="refresh"
      >
        Try again
      </button>
    </p>
    <ul v-if="rows.length" class="max-h-64 space-y-2 overflow-y-auto">
      <li
        v-for="file in rows"
        :key="file.key"
        class="flex flex-wrap items-end gap-2 rounded-lg bg-zinc-800/80 p-2"
      >
        <div class="min-w-0 flex-1 space-y-1">
          <label class="block space-y-1">
            <span
              class="block break-all px-1 text-sm font-medium text-zinc-100"
              >{{ file.name }}</span
            >
            <span v-if="file.source" class="block px-1 text-xs text-zinc-400"
              >{{ file.source.gameName }} · {{ file.source.versionName }} ·
              {{
                file.path
                  ? "Ready to use"
                  : inputStatus[file.source.status.type]
              }}</span
            >
            <span
              v-else-if="file.status"
              class="block px-1 text-xs text-zinc-400"
              >{{ file.status }}</span
            >
            <input
              v-if="file.path"
              :value="file.path"
              :title="file.path"
              readonly
              class="block w-full min-w-0 rounded-md border-0 bg-zinc-950/60 px-2 py-2 font-mono text-xs text-zinc-300 focus:ring-2 focus:ring-blue-400"
              @click="($event.currentTarget as HTMLInputElement).select()"
            />
          </label>
        </div>
        <button
          v-if="file.path"
          type="button"
          :aria-label="`Copy path for ${file.name}`"
          class="min-h-10 min-w-24 shrink-0 rounded-md bg-zinc-700 px-3 text-sm font-medium text-zinc-100 hover:bg-zinc-600 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400"
          @click="copyPath(file, $event)"
        >
          {{ copiedFile === file.key ? "Copied" : "Copy path" }}
        </button>
        <NuxtLink
          v-else-if="file.source"
          :to="{ path: '/store', query: { gameId: file.source.gameId } }"
          class="inline-flex min-h-10 items-center px-2 text-sm font-medium text-blue-300 hover:underline"
          >View source game</NuxtLink
        >
      </li>
    </ul>
    <button
      v-if="canRetry"
      type="button"
      :disabled="retrying"
      class="min-h-10 rounded-md bg-zinc-800 px-3 text-sm font-medium text-zinc-100 hover:bg-zinc-700 disabled:opacity-50"
      @click="retry"
    >
      {{ retrying ? "Checking companion files…" : "Download missing files" }}
    </button>
    <p
      v-if="downloadError"
      role="alert"
      class="whitespace-pre-line text-sm text-amber-200"
    >
      {{ downloadError }}
    </p>
    <p role="status" class="sr-only">
      {{
        copiedFile
          ? `Copied path for ${rows.find((file) => file.key === copiedFile)?.name ?? ""}`
          : ""
      }}
    </p>
  </section>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type CompanionFile = {
  name: string;
  path: string | null;
  asset_id: string | null;
  queued: boolean;
};
type LibraryInput = {
  gameId: string;
  gameName: string;
  versionId: string;
  versionName: string;
  kind: "file" | "folder";
  path: string;
  name?: string;
  copyPath: string | null;
  status: { type: keyof typeof inputStatus };
};
const inputStatus = {
  ready: "",
  downloadNeeded: "Not downloaded yet",
  wrongVersion: "A different version is installed",
  busy: "Source game is queued or in use",
  missing: "File or folder not found. Check the source game’s files.",
};
type FileRow = {
  key: string;
  name: string;
  path: string | null;
  source?: LibraryInput;
  status?: string;
};
const props = defineProps<{ gameId: string; running: boolean }>();
const files = ref<CompanionFile[]>([]);
const supportFiles = computed(() =>
  files.value.filter((file) => !file.asset_id),
);
const inputs = ref<LibraryInput[]>([]);
const rows = computed<FileRow[]>(() => [
  ...files.value
    .filter((file) => file.asset_id)
    .map((file) => ({
      ...file,
      key: file.asset_id ?? `bundled:${file.name}`,
      status: file.asset_id
        ? file.path
          ? "Ready to use"
          : file.queued
            ? "In the download queue"
            : "Not downloaded yet"
        : undefined,
    })),
  ...inputs.value.map((source) => ({
    key: `${source.versionId}:${source.kind}:${source.path}`,
    name: source.name || source.path || "Game folder",
    path: source.copyPath,
    source,
  })),
]);
const copiedFile = ref("");
const loadError = ref("");
const inputError = ref(false);
const copyError = ref("");
const downloadError = ref("");
const retrying = ref(false);
const canRetry = computed(
  () =>
    files.value.some((file) => file.asset_id && !file.path && !file.queued) ||
    inputs.value.some(
      ({ status }) =>
        status.type === "downloadNeeded" || status.type === "missing",
    ),
);
let request = 0;
const unlisteners: (() => void)[] = [];
let disposed = false;

async function refresh() {
  const id = ++request;
  const args = { id: props.gameId };
  const [bundled, reused] = await Promise.allSettled([
    invoke<CompanionFile[]>("fetch_installer_companion_files", args),
    invoke<LibraryInput[]>("fetch_installer_library_inputs", args),
  ]);
  if (disposed || id !== request) return;
  files.value = bundled.status === "fulfilled" ? bundled.value : [];
  inputs.value = reused.status === "fulfilled" ? reused.value : [];
  loadError.value =
    bundled.status === "rejected"
      ? "Could not read the downloaded files. Try opening the installer folder from the game menu."
      : "";
  inputError.value = reused.status === "rejected";
  if (
    !inputError.value &&
    !loadError.value &&
    inputs.value.every((input) => input.copyPath) &&
    files.value.every((file) => file.path)
  )
    downloadError.value = "";
}

async function retry() {
  retrying.value = true;
  downloadError.value = "";
  try {
    await invoke("retry_installer_companions", { id: props.gameId });
  } catch (cause) {
    downloadError.value = String(cause);
  } finally {
    retrying.value = false;
    void refresh();
  }
}

watch(() => props.running, refresh);
onMounted(() => {
  void refresh();
  window.addEventListener("focus", refresh);
  for (const listener of [
    listen("update_library", refresh),
    listen<string>("installer_assets_changed", ({ payload }) => {
      if (payload === props.gameId) void refresh();
    }),
    listen<[string, string]>("installer_companion_error", ({ payload }) => {
      if (payload[0] === props.gameId) downloadError.value = payload[1];
    }),
  ]) {
    void listener
      .then((stop) => {
        if (disposed) stop();
        else unlisteners.push(stop);
      })
      .catch(() => {});
  }
});
onUnmounted(() => {
  disposed = true;
  unlisteners.forEach((stop) => stop());
  window.removeEventListener("focus", refresh);
});

async function copyPath(file: FileRow, event: MouseEvent) {
  if (!file.path) return;
  const button = event.currentTarget as HTMLButtonElement;
  const input = button.closest("li")?.querySelector("input");
  copyError.value = "";
  copiedFile.value = "";
  try {
    if (!navigator.clipboard) throw new Error("Clipboard API unavailable");
    await navigator.clipboard.writeText(file.path);
  } catch {
    input?.focus();
    input?.select();
    try {
      // Some desktop webviews do not expose the async Clipboard API.
      if (!input || !document.execCommand("copy"))
        throw new Error("Copy failed");
      button.focus();
    } catch {
      copyError.value =
        "Could not copy automatically. Use your system’s copy shortcut on the selected path.";
      return;
    }
  }
  copiedFile.value = file.key;
}
</script>
