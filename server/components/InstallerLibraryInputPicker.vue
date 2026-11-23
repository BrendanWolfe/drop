<template>
  <details
    class="rounded-lg bg-zinc-900 p-3 text-sm text-zinc-200"
    @toggle="loadFirstGames"
  >
    <summary class="min-h-10 cursor-pointer py-2 font-medium text-zinc-100">
      {{ $t("library.admin.import.version.installer.libraryTitle") }}
      <span v-if="selection.length">{{ `(${selection.length})` }}</span>
    </summary>
    <p class="mb-3 text-zinc-400">
      {{ $t("library.admin.import.version.installer.libraryHelp") }}
    </p>
    <ul v-if="selection.length" class="mb-4 divide-y divide-zinc-800">
      <li
        v-for="(input, index) in selection"
        :key="`${input.versionId}:${input.path}`"
        class="flex items-center justify-between gap-2"
      >
        <span class="break-all py-2">
          {{
            $t("library.admin.import.version.installer.sourceSummary", {
              game: input.gameName,
              version: input.versionName,
              path:
                input.path ||
                $t("library.admin.import.version.installer.wholeFolder"),
            })
          }}
        </span>
        <button
          type="button"
          class="min-h-10 px-2 text-blue-300 hover:underline"
          @click="selection.splice(index, 1)"
        >
          {{ $t("library.admin.import.version.installer.remove") }}
        </button>
      </li>
    </ul>
    <div class="space-y-3" :aria-busy="busy">
      <form class="flex flex-wrap gap-2" @submit.prevent="searchGames">
        <input
          v-model="search"
          maxlength="200"
          class="field flex-1"
          :aria-label="$t('library.admin.import.version.installer.searchGames')"
          :placeholder="
            $t('library.admin.import.version.installer.searchGames')
          "
        />
        <button type="submit" :disabled="busy" class="action">
          {{ $t("library.admin.import.version.installer.search") }}
        </button>
      </form>
      <div class="grid gap-2 sm:grid-cols-2">
        <label>
          {{ $t("library.admin.import.version.installer.sourceGame") }}
          <select
            v-model="game"
            :disabled="busy"
            class="field mt-1 w-full"
            @change="loadVersions"
          >
            <option :value="null">
              {{ $t("library.admin.import.version.installer.chooseGame") }}
            </option>
            <option v-for="item in games" :key="item.id" :value="item">
              {{ item.mName }}
            </option>
          </select>
        </label>
        <label v-if="game">
          {{ $t("library.admin.import.version.installer.sourceVersion") }}
          <select
            v-model="version"
            :disabled="busy"
            class="field mt-1 w-full"
            @change="browse('')"
          >
            <option :value="null">
              {{ $t("library.admin.import.version.installer.chooseVersion") }}
            </option>
            <option
              v-for="item in versions"
              :key="item.versionId"
              :value="item"
            >
              {{ versionLabel(item) }}
            </option>
          </select>
        </label>
      </div>
      <div v-if="version" class="space-y-2">
        <div class="flex flex-wrap items-center gap-2">
          <p class="min-w-0 flex-1 break-all text-zinc-400">
            {{
              directory ||
              $t("library.admin.import.version.installer.wholeFolder")
            }}
          </p>
          <button
            v-if="directory"
            type="button"
            class="action"
            :disabled="busy"
            @click="browse(directory.split('/').slice(0, -1).join('/'))"
          >
            {{ $t("library.admin.import.version.installer.up") }}
          </button>
          <button
            type="button"
            class="action"
            :disabled="busy || selected(directory)"
            @click="add('folder', directory)"
          >
            {{ $t("library.admin.import.version.installer.useFolder") }}
          </button>
        </div>
        <form
          class="flex flex-wrap gap-2"
          @submit.prevent="browse(directory, fileSearch)"
        >
          <input
            v-model="fileSearch"
            maxlength="200"
            class="field flex-1"
            :aria-label="
              $t('library.admin.import.version.installer.searchFiles')
            "
            :placeholder="
              $t('library.admin.import.version.installer.searchFiles')
            "
          />
          <button type="submit" class="action" :disabled="busy">
            {{ $t("library.admin.import.version.installer.search") }}
          </button>
        </form>
        <p v-if="more" class="text-zinc-400">
          {{ $t("library.admin.import.version.installer.moreFiles") }}
        </p>
        <p v-if="!entries.length && !busy && !error" class="text-zinc-400">
          {{ $t("library.admin.import.version.installer.noMatches") }}
        </p>
        <ul class="max-h-52 divide-y divide-zinc-800 overflow-y-auto">
          <li
            v-for="entry in entries"
            :key="entry.name"
            class="flex items-center justify-between gap-2"
          >
            <button
              v-if="entry.kind === 'folder'"
              type="button"
              class="min-h-10 min-w-0 break-all px-2 text-left text-blue-300 hover:underline"
              :disabled="busy"
              @click="browse(entryPath(entry.name))"
            >
              {{ `${entry.name}/` }}
            </button>
            <template v-else>
              <span class="min-w-0 break-all px-2">{{ entry.name }}</span>
              <button
                type="button"
                class="min-h-10 shrink-0 px-2 text-blue-300 hover:underline disabled:text-zinc-500"
                :disabled="busy || selected(entryPath(entry.name))"
                @click="add('file', entryPath(entry.name))"
              >
                {{ $t("library.admin.import.version.installer.useFileInput") }}
              </button>
            </template>
          </li>
        </ul>
      </div>
      <p v-if="busy" role="status" class="text-zinc-400">
        {{ $t("library.admin.import.version.installer.loading") }}
      </p>
      <p v-else-if="error" role="alert" class="text-red-300">{{ error }}</p>
      <p v-else-if="!games.length" class="text-zinc-400">
        {{ $t("library.admin.import.version.installer.noGames") }}
      </p>
    </div>
  </details>
</template>

<script setup lang="ts">
import { ofetch } from "ofetch";
import type { LibraryInput } from "~/server/internal/library/installer-input-paths";

type Game = { id: string; mName: string };
type Version = {
  versionId: string;
  displayName: string | null;
  versionPath: string | null;
};
type Entry = { kind: LibraryInput["kind"]; name: string };
type PickerResponse = {
  games?: Game[];
  versions?: Version[];
  entries?: Entry[];
  more?: boolean;
};
const props = defineProps<{ gameId: string }>();
const selection = defineModel<
  (LibraryInput & { gameName: string; versionName: string })[]
>({ required: true });
const { t } = useI18n();
const search = ref("");
const fileSearch = ref("");
const games = ref<Game[]>([]);
const versions = ref<Version[]>([]);
const game = ref<Game | null>(null);
const version = ref<Version | null>(null);
const directory = ref("");
const entries = ref<Entry[]>([]);
const more = ref(false);
const busy = ref(false);
const error = ref("");
let started = false;

function versionLabel(item: Version) {
  return item.displayName || item.versionPath || item.versionId;
}
function entryPath(name: string) {
  return directory.value ? `${directory.value}/${name}` : name;
}
function selected(path: string) {
  return (
    selection.value.length >= 24 ||
    selection.value.some(
      (item) =>
        item.versionId === version.value?.versionId && item.path === path,
    )
  );
}
function add(kind: LibraryInput["kind"], path: string) {
  if (!game.value || !version.value || selected(path)) return;
  selection.value.push({
    gameId: game.value.id,
    versionId: version.value.versionId,
    kind,
    path,
    gameName: game.value.mName,
    versionName: versionLabel(version.value),
  });
}

function loadFirstGames(event: Event) {
  if (!(event.currentTarget as HTMLDetailsElement).open || started) return;
  started = true;
  void searchGames();
}

async function request(
  query: Record<string, string>,
  apply: (data: PickerResponse) => void,
) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    apply(
      await ofetch<PickerResponse>(
        "/api/v1/admin/import/version/installer-library",
        { query: { owner: props.gameId, ...query } },
      ),
    );
  } catch {
    error.value = t("library.admin.import.version.installer.browseError");
  } finally {
    busy.value = false;
  }
}

async function searchGames() {
  await request({ search: search.value }, (data) => {
    game.value = version.value = null;
    games.value = data.games ?? [];
  });
}

async function loadVersions() {
  version.value = null;
  versions.value = [];
  if (game.value)
    await request({ source: game.value.id }, (data) => {
      versions.value = data.versions ?? [];
    });
}

async function browse(folder: string, search = "") {
  if (!game.value || !version.value) return;
  directory.value = folder;
  fileSearch.value = search;
  entries.value = [];
  more.value = false;
  await request(
    {
      source: game.value.id,
      version: version.value.versionId,
      directory: folder,
      search,
    },
    (data) => {
      entries.value = data.entries ?? [];
      more.value = !!data.more;
    },
  );
}
</script>

<style scoped>
@reference "../assets/tailwindcss.css";

.field {
  @apply min-h-10 min-w-0 rounded-md border-zinc-700 bg-zinc-950 text-sm text-zinc-100;
}
.action {
  @apply min-h-10 rounded-md bg-zinc-800 px-3 text-sm text-zinc-200 hover:bg-zinc-700 disabled:opacity-50;
}
</style>
