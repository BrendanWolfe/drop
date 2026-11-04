<template>
  <ModalTemplate v-model="visible" size-class="max-w-2xl">
    <template #default>
      <h2 class="text-xl font-display font-semibold text-zinc-100">
        Add a game
      </h2>
      <div
        class="my-5 flex gap-1 rounded-lg bg-zinc-950 p-1"
        role="group"
        aria-label="Game source"
      >
        <button
          v-for="option in modes"
          :key="option.id"
          type="button"
          :aria-pressed="mode === option.id"
          class="flex-1 rounded-md px-3 py-2 text-sm font-medium focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400"
          :class="
            mode === option.id
              ? 'bg-zinc-800 text-white'
              : 'text-zinc-400 hover:text-zinc-200'
          "
          @click="mode = option.id"
        >
          {{ option.label }}
        </button>
      </div>
      <LocalGameForm
        v-if="mode === 'existing'"
        @saved="added"
        @cancel="visible = false"
      />
      <form v-else class="space-y-4" @submit.prevent="download">
        <p class="text-sm text-zinc-400">
          Download a ZIP, 7z, RAR, or tar archive, extract it, then choose the
          file to launch. Use a direct download link, not a web page.
        </p>
        <div>
          <label
            for="archive-name"
            class="mb-1.5 block text-sm font-medium text-zinc-200"
            >Game name</label
          ><input
            id="archive-name"
            v-model="name"
            required
            maxlength="200"
            class="archive-input"
          />
        </div>
        <div>
          <label
            for="archive-url"
            class="mb-1.5 block text-sm font-medium text-zinc-200"
            >Archive URL</label
          ><input
            id="archive-url"
            v-model="url"
            type="url"
            required
            class="archive-input"
            placeholder="https://example.com/game.rar"
            autocomplete="off"
            spellcheck="false"
          />
        </div>
        <InstallDirectorySelector
          v-if="directories.length"
          :install-dirs="directories"
          v-model="installDir"
        />
        <p class="text-xs text-zinc-500">
          Extracts directly into a new folder. Drop credentials are never sent
          to the download host. Password-protected and multipart archives are
          not supported.
        </p>
        <p
          v-if="error"
          role="alert"
          class="rounded-md bg-red-500/10 p-3 text-sm text-red-300"
        >
          {{ error }}
        </p>
        <div class="flex justify-end gap-2 pt-2">
          <button
            type="button"
            class="rounded-md bg-zinc-800 px-3 py-2 text-sm text-zinc-200"
            @click="visible = false"
          >
            Cancel
          </button>
          <button
            type="submit"
            :disabled="busy || !directories.length"
            class="rounded-md bg-blue-600 px-4 py-2 text-sm font-semibold text-white hover:bg-blue-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-50"
          >
            {{ busy ? "Adding…" : "Download archive" }}
          </button>
        </div>
      </form>
    </template>
    <template #buttons><span /></template>
  </ModalTemplate>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
const visible = defineModel<boolean>();
const mode = ref<"existing" | "url">("existing");
const modes = [
  { id: "existing", label: "Add existing game" },
  { id: "url", label: "Download from URL" },
] as const;
const name = ref("");
const url = ref("");
const installDir = ref(0);
const busy = ref(false);
const error = ref("");
const directories = ref<string[]>([]);
const router = useRouter();
onMounted(async () => {
  try {
    directories.value = await invoke<string[]>("fetch_download_dir_stats");
  } catch (e) {
    error.value = String(e);
  }
});
async function added(id: string) {
  visible.value = false;
  await router.push(`/library/local/${id}`);
}
async function download() {
  busy.value = true;
  error.value = "";
  try {
    await added(
      await invoke<string>("download_archive", {
        name: name.value,
        url: url.value,
        installDir: installDir.value,
      }),
    );
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>
<style scoped>
.archive-input {
  @apply block w-full rounded-md border-0 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 ring-1 ring-inset ring-zinc-700 focus:ring-2 focus:ring-blue-500;
}
</style>
