<template>
  <section class="space-y-4 rounded-xl bg-zinc-800 p-4">
    <div>
      <h3 class="text-sm font-medium text-zinc-100">{{ label("title") }}</h3>
      <p class="mt-1 text-sm text-zinc-400">
        {{ label("help") }}
      </p>
    </div>
    <div class="flex flex-wrap items-center gap-2 text-sm text-zinc-200">
      <EmulatorWidget v-if="emulator" :emulator="emulator" />
      <LoadingButton :loading="false" @click="selectEmulator = true">{{
        emulator ? "Change play emulator" : "Select play emulator"
      }}</LoadingButton>
      <button
        v-if="emulator"
        type="button"
        class="min-h-10 px-2 text-blue-300 hover:underline"
        @click="setEmulator(undefined)"
      >
        Remove
      </button>
    </div>
    <ModalSelectLaunch
      v-model="selectEmulator"
      :filter-platform="model.platform"
      @select="setEmulator"
    />
    <div class="flex flex-wrap items-center gap-4 text-sm text-zinc-200">
      <label>
        {{ $t("library.admin.import.version.platform") }}
        <select
          v-model="model.platform"
          class="ml-2 rounded-md border-zinc-700 bg-zinc-950 text-sm"
        >
          <option v-for="platform in platforms" :key="platform">
            {{ platform }}
          </option>
        </select>
      </label>
      <label
        v-if="model.platform === 'Windows'"
        class="flex min-h-10 items-center gap-2"
      >
        <input
          v-model="model.protonCompatible"
          type="checkbox"
          class="rounded border-zinc-600 bg-zinc-950"
        />
        {{ label("proton") }}
      </label>
    </div>
    <p class="break-all text-sm text-zinc-300">
      <strong>{{ label("executable") }}</strong>
      {{ model.executable || label("notSelected") }}
    </p>
    <div class="text-sm text-zinc-300">
      <strong>{{ label("supportFiles") }}</strong>
      <p v-if="!model.files.length" class="text-zinc-400">
        {{ label("noSupportFiles") }}
      </p>
      <ul v-else>
        <li
          v-for="file in model.files"
          :key="file"
          class="flex items-center justify-between gap-3"
        >
          <span class="break-all">{{ file }}</span>
          <button
            type="button"
            class="min-h-10 shrink-0 px-2 text-blue-300 hover:underline"
            :aria-label="label('removeFile', { name: file })"
            @click="remove(file)"
          >
            {{ label("remove") }}
          </button>
        </li>
      </ul>
    </div>
    <div class="rounded-lg bg-zinc-900 p-3">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <p class="break-all text-sm text-zinc-300">
          {{ label("libraryPath", { path: directory }) }}
        </p>
        <button
          v-if="directory"
          type="button"
          class="min-h-10 rounded-md bg-zinc-800 px-3 text-sm text-zinc-200 hover:bg-zinc-700"
          :disabled="loading"
          @click="up"
        >
          {{ label("up") }}
        </button>
      </div>
      <p v-if="loading" role="status" class="p-2 text-sm text-zinc-400">
        {{ label("loading") }}
      </p>
      <p v-else-if="error" role="alert" class="p-2 text-sm text-red-300">
        {{ error }}
      </p>
      <p v-else-if="!entries.length" class="p-2 text-sm text-zinc-400">
        {{ label("empty") }}
      </p>
      <ul v-else class="max-h-64 divide-y divide-zinc-800 overflow-y-auto">
        <li
          v-for="entry in entries"
          :key="entry.name"
          class="flex flex-wrap items-center justify-between gap-x-2 text-sm"
        >
          <button
            v-if="entry.directory"
            type="button"
            class="min-h-10 min-w-0 flex-1 break-all px-2 text-left text-blue-300 hover:bg-zinc-800"
            @click="browse(entryPath(entry.name))"
          >
            {{ `${entry.name}/` }}
          </button>
          <template v-else>
            <span class="min-w-0 break-all px-2 text-zinc-200">{{
              entry.name
            }}</span>
            <div class="flex shrink-0 gap-1">
              <button
                type="button"
                :aria-label="label('useFile', { name: entry.name })"
                :disabled="model.executable === entryPath(entry.name)"
                class="min-h-10 px-2 text-blue-300 hover:underline disabled:text-zinc-500"
                @click="chooseInstaller(entryPath(entry.name))"
              >
                {{ label("use") }}
              </button>
              <button
                type="button"
                :aria-label="label('addSupportFile', { name: entry.name })"
                :disabled="
                  model.executable === entryPath(entry.name) ||
                  model.files.includes(entryPath(entry.name)) ||
                  model.files.length >= 1000
                "
                class="min-h-10 px-2 text-blue-300 hover:underline disabled:text-zinc-500"
                @click="model.files.push(entryPath(entry.name))"
              >
                {{ label("addSupport") }}
              </button>
            </div>
          </template>
        </li>
      </ul>
    </div>
  </section>
</template>

<script setup lang="ts">
import type { ImportVersion } from "~/server/api/v1/admin/import/version/index.post";
import type { EmulatorLaunchObject } from "~/composables/frontend";

const props = defineProps<{ gameId: string }>();
const { t } = useI18n();
const platforms = ["Windows", "Linux", "macOS"] as const;
// All keys are defined in the installer's locale section.
const label = (key: string, values: Record<string, string> = {}) =>
  // eslint-disable-next-line @intlify/vue-i18n/no-dynamic-keys
  t(`library.admin.import.version.installer.${key}`, values);
const model = defineModel<
  NonNullable<typeof ImportVersion.infer.installerPackage>
>({ required: true });
const directory = ref("");
const emulator = ref<EmulatorLaunchObject>();
const selectEmulator = ref(false);
function setEmulator(value?: EmulatorLaunchObject) {
  emulator.value = value;
  model.value.emulatorId = value?.launchId;
}
watch(
  () => model.value.platform,
  () => setEmulator(),
);
const entries = ref<{ name: string; directory: boolean; zip?: boolean }[]>([]);
const loading = ref(false);
const error = ref("");

function entryPath(name: string) {
  return directory.value ? `${directory.value}/${name}` : name;
}

function remove(file: string) {
  model.value.files = model.value.files.filter((item) => item !== file);
}

function chooseInstaller(file: string) {
  model.value.executable = file;
  remove(file);
}

function up() {
  return browse(directory.value.split("/").slice(0, -1).join("/"));
}

async function browse(folder?: string) {
  loading.value = true;
  error.value = "";
  try {
    const result = await $dropFetch(
      "/api/v1/admin/import/version/installer-files",
      { query: { id: props.gameId, directory: folder } },
    );
    directory.value = result.directory;
    entries.value = result.entries;
  } catch {
    error.value = label("browseError");
  } finally {
    loading.value = false;
  }
}

onMounted(() => browse());
</script>
