<template>
  <form class="mx-auto w-full max-w-4xl space-y-6 p-6" @submit.prevent="save">
    <header>
      <h2 class="font-display text-xl font-semibold text-zinc-100">
        {{ $t("cloudSaves.title") }}
      </h2>
      <p class="mt-2 max-w-2xl text-sm text-zinc-400">
        {{ $t("cloudSaves.description") }}
      </p>
    </header>
    <p v-if="game.type !== 'Game'" class="text-sm text-zinc-400">
      {{ $t("cloudSaves.gamesOnly") }}
    </p>
    <fieldset v-else :disabled="saving || loading" class="space-y-6">
      <label
        class="flex min-h-10 items-center gap-3 text-sm font-medium text-zinc-100"
      >
        <input
          v-model="enabled"
          type="checkbox"
          class="rounded border-zinc-600 bg-zinc-800 text-blue-600 focus:ring-blue-500"
        />
        {{ $t("cloudSaves.enable") }}
      </label>
      <div class="rounded-lg bg-zinc-800/50 p-4 text-sm text-zinc-300">
        <p>
          {{ $t("cloudSaves.pathHelp") }}
          <code>{{ examplePath }}</code>
        </p>
        <p class="mt-2 text-zinc-400">
          {{ $t("cloudSaves.supportedRoots") }}
          <span class="mt-1 flex flex-wrap gap-x-3 gap-y-1"
            ><code v-for="root in placeholders" :key="root">{{
              root
            }}</code></span
          >
        </p>
        <p class="mt-2 text-zinc-400">
          {{ $t("cloudSaves.directoryHelp") }}
        </p>
      </div>
      <section
        v-for="(location, index) in locations"
        :key="index"
        class="space-y-4 rounded-xl bg-zinc-800/50 p-4"
      >
        <div class="flex items-center justify-between gap-3">
          <h3 class="text-sm font-semibold text-zinc-100">
            {{ $t("cloudSaves.location", { index: index + 1 }) }}
          </h3>
          <button
            type="button"
            class="min-h-10 rounded-md px-3 text-sm text-red-300 hover:bg-zinc-700 focus-visible:outline focus-visible:outline-blue-400"
            @click="locations.splice(index, 1)"
          >
            {{ $t("common.remove") }}
          </button>
        </div>
        <div class="grid gap-4 sm:grid-cols-3">
          <label class="text-sm text-zinc-300"
            >{{ $t("cloudSaves.locationId") }}
            <input
              v-model="location.id"
              required
              maxlength="40"
              pattern="[a-zA-Z0-9_-]+"
              class="save-input"
              placeholder="profiles"
            />
          </label>
          <label class="text-sm text-zinc-300"
            >{{ $t("cloudSaves.platform") }}
            <select v-model="location.platform" class="save-input">
              <option
                v-for="platform in platforms"
                :key="platform"
                :value="platform"
              >
                {{ platform }}
              </option>
            </select>
          </label>
          <label class="text-sm text-zinc-300"
            >{{ $t("cloudSaves.type") }}
            <select v-model="location.type" class="save-input">
              <option value="directory">
                {{ $t("cloudSaves.directory") }}
              </option>
              <option value="file">{{ $t("cloudSaves.file") }}</option>
            </select>
          </label>
        </div>
        <label class="block text-sm text-zinc-300"
          >{{ $t("cloudSaves.path") }}
          <input
            v-model="location.path"
            required
            maxlength="1024"
            class="save-input font-mono"
            placeholder="<winLocalAppData>/ExampleGame/Saves"
          />
        </label>
        <label class="block text-sm text-zinc-300"
          >{{ $t("cloudSaves.exclusions") }}
          <textarea
            :value="location.exclude.join('\n')"
            rows="2"
            class="save-input font-mono"
            placeholder="Cache/**&#10;**/*.lock"
            @change="
              location.exclude = ($event.target as HTMLTextAreaElement).value
                .split('\n')
                .map((s) => s.trim())
                .filter(Boolean)
            "
          />
        </label>
      </section>
      <div class="flex flex-wrap gap-3">
        <button
          type="button"
          :disabled="locations.length >= 16"
          class="min-h-10 rounded-md bg-zinc-800 px-4 text-sm font-semibold text-zinc-100 hover:bg-zinc-700 disabled:opacity-50"
          @click="add"
        >
          {{ $t("cloudSaves.addLocation") }}
        </button>
        <LoadingButton type="submit" :loading="saving" class="min-h-10">{{
          $t("cloudSaves.save")
        }}</LoadingButton>
      </div>
      <p class="text-xs text-zinc-400">
        {{ $t("cloudSaves.disableHelp") }}
      </p>
    </fieldset>
    <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
    <p v-if="saved" role="status" class="text-sm text-green-300">
      {{ $t("cloudSaves.saved") }}
    </p>
  </form>
</template>

<script setup lang="ts">
import type { GameModel } from "~/prisma/client/models";
import type { SaveLocation } from "~/server/internal/cloud-saves/validation";

const game = defineModel<GameModel>({ required: true });
const { t } = useI18n();
const examplePath = "<winLocalAppData>/ExampleGame/Saves";
const placeholders = [
  "<base>",
  "<home>",
  "<winAppData>",
  "<winLocalAppData>",
  "<winLocalAppDataLow>",
  "<winDocuments>",
  "<xdgData>",
  "<xdgConfig>",
];
const platforms = ["Windows", "Linux", "macOS"];
const enabled = ref(false);
const locations = ref<SaveLocation[]>([]);
const loading = ref(true);
const revision = ref(game.value.cloudSaveRevision);
const saving = ref(false);
const saved = ref(false);
const error = ref("");
onMounted(async () => {
  try {
    const config = await $dropFetch("/api/v1/admin/game/:id/cloud-saves", {
      params: { id: game.value.id },
    });
    enabled.value = config.enabled;
    locations.value = config.locations;
    revision.value = config.revision;
    loading.value = false;
  } catch {
    error.value = t("cloudSaves.saveFailed");
  }
});
function add() {
  let index = 1;
  while (locations.value.some((location) => location.id === `saves-${index}`))
    index++;
  locations.value.push({
    id: `saves-${index}`,
    platform: "Windows",
    path: "",
    type: "directory",
    exclude: [],
  });
}
async function save() {
  saving.value = true;
  saved.value = false;
  error.value = "";
  try {
    const result = await $dropFetch("/api/v1/admin/game/:id/cloud-saves", {
      method: "PATCH",
      params: { id: game.value.id },
      body: {
        enabled: enabled.value,
        locations: locations.value,
        revision: revision.value,
      },
    });
    Object.assign(game.value, result);
    revision.value = result.cloudSaveRevision;
    saved.value = true;
  } catch (cause) {
    error.value =
      (cause as { data?: { message?: string } }).data?.message ??
      t("cloudSaves.saveFailed");
  } finally {
    saving.value = false;
  }
}
</script>

<style scoped>
@reference "../../assets/tailwindcss.css";

.save-input {
  @apply mt-2 block min-h-10 w-full rounded-md border-zinc-700 bg-zinc-900 text-sm text-zinc-100 focus:border-blue-500 focus:ring-blue-500;
}
</style>
