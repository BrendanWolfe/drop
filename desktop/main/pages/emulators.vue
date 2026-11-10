<template>
  <div class="flex h-full min-h-0 flex-col text-zinc-100 sm:flex-row">
    <aside
      aria-label="Emulator library"
      class="flex max-h-64 shrink-0 flex-col border-b border-zinc-800/50 bg-zinc-950/50 px-4 py-3 backdrop-blur-xl sm:max-h-full sm:w-60 sm:border-b-0 sm:border-r md:w-72"
    >
      <div class="mb-4 flex shrink-0 items-center gap-2">
        <div class="relative min-w-0 grow">
          <MagnifyingGlassIcon
            class="pointer-events-none absolute left-3 top-3 size-4 text-zinc-400"
            aria-hidden="true"
          />
          <input
            v-model="searchQuery"
            type="search"
            aria-label="Search installed emulators"
            placeholder="Search emulators…"
            class="block min-h-10 w-full rounded-lg border-0 bg-zinc-800/50 py-2 pl-9 pr-3 text-sm text-zinc-100 placeholder:text-zinc-500 focus:bg-zinc-800 focus:ring-2 focus:ring-inset focus:ring-blue-500"
          />
        </div>
        <button
          type="button"
          class="emulator-button size-10 shrink-0 !px-0"
          :disabled="loading"
          aria-label="Refresh installed emulators"
          title="Refresh installed emulators"
          @click="refresh"
        >
          <ArrowPathIcon
            class="size-4"
            :class="{ 'motion-safe:animate-spin': loading }"
            aria-hidden="true"
          />
        </button>
      </div>
      <div class="mb-2 flex items-center justify-between gap-2">
        <h1 class="font-display text-sm font-semibold">Emulators</h1>
        <span class="text-xs tabular-nums text-zinc-500"
          >{{ library.length }} installed</span
        >
      </div>
      <nav
        class="min-h-0 overflow-y-auto"
        aria-label="Installed emulators"
        :aria-busy="loading"
      >
        <ul class="space-y-1.5">
          <li v-for="entry in filteredLibrary" :key="entry.gameId">
            <button
              type="button"
              :aria-current="
                activeEmulator?.gameId === entry.gameId ? 'true' : undefined
              "
              aria-controls="emulator-details"
              class="flex min-h-12 w-full items-center gap-3 rounded-lg px-2 py-2 text-left transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400"
              :class="
                activeEmulator?.gameId === entry.gameId
                  ? 'bg-zinc-800 text-zinc-100 shadow-md shadow-zinc-950/20'
                  : 'text-zinc-300 hover:bg-zinc-800/70 hover:text-zinc-100'
              "
              @click="selectEmulator(entry.gameId)"
            >
              <img
                v-if="entry.iconObjectId && !failedIcons[entry.gameId]"
                :src="useObject(entry.iconObjectId)"
                alt=""
                class="size-8 shrink-0 rounded object-cover outline outline-1 -outline-offset-1 outline-white/10"
                @error="failedIcons[entry.gameId] = true"
              />
              <span
                v-else
                class="flex size-8 shrink-0 items-center justify-center rounded bg-zinc-700/50 font-display text-sm font-semibold text-zinc-300"
                aria-hidden="true"
                >{{ entry.name.slice(0, 1).toLocaleUpperCase() }}</span
              >
              <span class="min-w-0">
                <span
                  class="block truncate font-display text-sm font-semibold"
                  >{{ entry.name }}</span
                >
                <span class="block text-xs tabular-nums text-zinc-400"
                  >{{ entry.versions.length }}
                  {{ entry.versions.length === 1 ? "version" : "versions" }} ·
                  {{ entry.requiredGameCount }}
                  {{ entry.requiredGameCount === 1 ? "game" : "games" }}</span
                >
              </span>
            </button>
          </li>
        </ul>
        <p
          v-if="loading && !library.length"
          role="status"
          class="py-4 text-sm text-zinc-400"
        >
          Loading emulators…
        </p>
        <p
          v-else-if="library.length && !filteredLibrary.length"
          class="py-4 text-sm text-zinc-400"
        >
          No emulators match “{{ searchQuery }}”.
        </p>
      </nav>
    </aside>

    <main
      id="emulator-details"
      ref="detailsPanel"
      class="min-h-0 min-w-0 grow overflow-y-auto"
    >
      <div
        v-if="error"
        role="alert"
        class="m-6 rounded-lg bg-red-950/40 p-4 text-sm text-red-300"
      >
        {{ error }}
      </div>
      <template v-if="activeEmulator">
        <header
          class="relative isolate overflow-hidden px-6 pb-8 pt-32 md:px-8 md:pt-44"
        >
          <div
            class="pointer-events-none absolute inset-0 -z-10"
            aria-hidden="true"
          >
            <img
              v-if="
                activeEmulator.bannerObjectId &&
                !failedBanners[activeEmulator.gameId]
              "
              :src="useObject(activeEmulator.bannerObjectId)"
              alt=""
              class="h-full w-full scale-105 object-cover blur-sm"
              @error="failedBanners[activeEmulator.gameId] = true"
            />
            <div v-else class="h-full bg-zinc-800/40" />
            <div
              class="absolute inset-0 bg-gradient-to-t from-zinc-900 via-zinc-900/80 to-zinc-900/20"
            />
            <div
              class="absolute inset-0 bg-gradient-to-r from-zinc-900/90 to-transparent"
            />
          </div>
          <p
            class="mb-3 text-xs font-semibold uppercase tracking-wider text-zinc-400"
          >
            Installed emulator
          </p>
          <h2
            class="break-words text-balance font-display text-4xl font-bold tracking-tight drop-shadow-lg md:text-5xl"
          >
            {{ activeEmulator.name }}
          </h2>
          <p
            v-if="activeEmulator.shortDescription"
            class="mt-3 max-w-2xl text-pretty text-sm leading-6 text-zinc-300"
          >
            {{ activeEmulator.shortDescription }}
          </p>
          <p class="mt-4 text-sm tabular-nums text-zinc-400">
            {{ activeEmulator.versions.length }} installed
            {{ activeEmulator.versions.length === 1 ? "version" : "versions"
            }}<span class="mx-2 text-zinc-600" aria-hidden="true">·</span
            >{{ activeEmulator.requiredGameCount }}
            {{
              activeEmulator.requiredGameCount === 1
                ? "game relies"
                : "games rely"
            }}
            on this emulator
          </p>
        </header>

        <section
          class="px-6 pb-8 md:px-8"
          aria-labelledby="installed-versions-heading"
        >
          <h3
            id="installed-versions-heading"
            class="font-display text-xl font-semibold"
          >
            Installed versions
          </h3>
          <p class="mt-2 max-w-2xl text-pretty text-sm leading-6 text-zinc-400">
            Games automatically use the exact version they need. Different
            versions stay installed side by side.
          </p>
          <ul class="mt-5 space-y-4" aria-label="Installed versions">
            <li
              v-for="version in activeEmulator.versions"
              :key="`${version.versionId}:${version.platform}`"
              class="rounded-xl bg-zinc-800/50 p-5 md:p-6"
            >
              <div class="flex flex-wrap items-start justify-between gap-4">
                <div class="min-w-0">
                  <h4 class="break-all font-display text-lg font-semibold">
                    {{ version.versionName }}
                  </h4>
                  <p
                    class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-zinc-400"
                  >
                    <span>{{ version.platform }}</span>
                    <span
                      class="inline-flex items-center gap-1.5"
                      :class="
                        emulatorVersionStatus(version) === 'Installed'
                          ? 'text-green-400'
                          : 'text-amber-200'
                      "
                      :role="version.busy ? 'status' : undefined"
                    >
                      <span
                        class="size-1.5 rounded-full bg-current"
                        aria-hidden="true"
                      />{{ emulatorVersionStatus(version) }}
                    </span>
                  </p>
                </div>
                <div class="flex flex-wrap gap-2">
                  <button
                    v-if="version.installType.type === 'PartiallyInstalled'"
                    type="button"
                    class="emulator-button size-10 !px-0"
                    :disabled="
                      version.busy || resuming[dependentHeadingId(version)]
                    "
                    :aria-label="`Resume download of ${version.versionName}`"
                    title="Resume download"
                    @click="resumeVersion(version)"
                  >
                    <ArrowDownTrayIcon class="size-4" aria-hidden="true" />
                  </button>
                  <button
                    v-if="version.directOpenLaunches?.length === 1"
                    type="button"
                    class="emulator-button size-10 !px-0"
                    :disabled="!canOpenEmulator(version)"
                    :aria-label="`Open ${activeEmulator.name} ${version.versionName} without a game`"
                    title="Open emulator"
                    @click="
                      openEmulator(
                        version,
                        version.directOpenLaunches[0]!.launchId,
                      )
                    "
                  >
                    <ArrowTopRightOnSquareIcon
                      class="size-4"
                      aria-hidden="true"
                    />
                  </button>
                  <Menu
                    v-else-if="(version.directOpenLaunches?.length ?? 0) > 1"
                    as="div"
                    class="relative"
                  >
                    <MenuButton
                      class="emulator-button size-10 !px-0"
                      :disabled="!canOpenEmulator(version)"
                      :aria-label="`Open ${activeEmulator.name} ${version.versionName} without a game`"
                      title="Open emulator"
                    >
                      <ArrowTopRightOnSquareIcon
                        class="size-4"
                        aria-hidden="true"
                      />
                    </MenuButton>
                    <MenuItems
                      class="absolute right-0 z-10 mt-2 min-w-48 rounded-lg bg-zinc-800 p-1 shadow-xl ring-1 ring-white/10 focus:outline-none"
                    >
                      <MenuItem
                        v-for="launch in version.directOpenLaunches"
                        :key="launch.launchId"
                        v-slot="{ active }"
                      >
                        <button
                          type="button"
                          class="min-h-10 w-full rounded-md px-3 py-2 text-left text-sm"
                          :class="{ 'bg-zinc-700': active }"
                          :disabled="!canOpenEmulator(version)"
                          @click="openEmulator(version, launch.launchId)"
                        >
                          {{ launch.name }}
                        </button>
                      </MenuItem>
                    </MenuItems>
                  </Menu>
                  <button
                    type="button"
                    class="emulator-button size-10 !px-0"
                    :aria-label="`Open folder for ${version.versionName}`"
                    title="Open folder"
                    @click="openFolder(version)"
                  >
                    <FolderOpenIcon class="size-4" aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    class="emulator-button size-10 !px-0 !text-red-300 hover:enabled:!text-red-200"
                    :disabled="version.busy"
                    :aria-label="`Uninstall ${version.versionName}`"
                    title="Uninstall version"
                    :aria-describedby="
                      version.requiredBy.length
                        ? dependentHeadingId(version)
                        : undefined
                    "
                    @click="requestUninstall(version)"
                  >
                    <TrashIcon class="size-4" aria-hidden="true" />
                  </button>
                </div>
              </div>

              <div class="mt-6 border-t border-white/5 pt-5">
                <h5
                  :id="dependentHeadingId(version)"
                  class="flex items-center gap-2 text-sm font-semibold"
                >
                  Games using this version
                  <span
                    class="rounded bg-zinc-700/60 px-2 py-0.5 text-xs tabular-nums text-zinc-300"
                    >{{ version.requiredBy.length }}</span
                  >
                </h5>
                <ul
                  v-if="version.requiredBy.length"
                  class="mt-3 divide-y divide-white/5"
                >
                  <li v-for="game in version.requiredBy" :key="game.gameId">
                    <NuxtLink
                      :to="`/library/${game.gameId}`"
                      class="group -mx-2 flex min-h-11 items-center justify-between gap-3 rounded-md px-2 py-2 transition-colors hover:bg-zinc-700/40 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400"
                    >
                      <span class="min-w-0">
                        <span
                          class="block truncate text-sm font-medium text-blue-300 group-hover:text-blue-200"
                          >{{ game.name }}</span
                        >
                        <span class="block text-xs text-zinc-400">{{
                          game.versionName
                        }}</span>
                      </span>
                      <ChevronRightIcon
                        class="size-4 shrink-0 text-zinc-500 group-hover:text-zinc-300"
                        aria-hidden="true"
                      />
                    </NuxtLink>
                  </li>
                </ul>
                <p v-else class="mt-3 text-sm text-zinc-400">
                  No installed games need this version. You can uninstall it
                  without affecting your games.
                </p>
                <p
                  v-if="version.requiredBy.length"
                  class="mt-3 text-pretty text-xs leading-5 text-zinc-400"
                >
                  Uninstalling this version keeps your game files, but these
                  games will need to download the emulator again before playing.
                </p>
              </div>

              <div class="mt-5 border-t border-white/5 pt-4">
                <p class="text-xs font-medium text-zinc-400">
                  Installation folder
                </p>
                <p
                  class="mt-1 break-all font-mono text-xs leading-5 text-zinc-500"
                >
                  {{ version.installDir }}
                </p>
              </div>
            </li>
          </ul>
        </section>
      </template>
      <div
        v-else-if="!loading && !error"
        class="flex min-h-full flex-col items-center justify-center px-6 py-16 text-center"
      >
        <CpuChipIcon class="mb-5 size-10 text-zinc-500" aria-hidden="true" />
        <h2 class="font-display text-2xl font-semibold">
          No emulators installed
        </h2>
        <p class="mt-3 max-w-sm text-pretty text-sm leading-6 text-zinc-400">
          Install an emulated game from the store. Drop downloads the emulator
          it needs automatically.
        </p>
        <NuxtLink to="/store" class="emulator-button mt-6"
          >Browse store</NuxtLink
        >
      </div>
    </main>

    <EmulatorUninstallModal
      v-if="uninstallTarget && confirmOpen"
      v-model="confirmOpen"
      :emulator="uninstallTarget"
      @uninstalled="refresh"
    />
  </div>
</template>

<script setup lang="ts">
import {
  ArrowDownTrayIcon,
  ArrowPathIcon,
  ArrowTopRightOnSquareIcon,
  ChevronRightIcon,
  CpuChipIcon,
  FolderOpenIcon,
  MagnifyingGlassIcon,
  TrashIcon,
} from "@heroicons/vue/20/solid";
import { Menu, MenuButton, MenuItem, MenuItems } from "@headlessui/vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { InstalledEmulator } from "~/types";
import {
  emulatorLibrary,
  emulatorCanOpen,
  emulatorSelection,
  emulatorVersionStatus,
  filterEmulatorLibrary,
} from "~/utils/emulator-library";

useHead({ title: "Emulators" });
const emulators = ref<InstalledEmulator[]>([]);
const loading = ref(true);
const error = ref("");
const searchQuery = ref("");
const selectedGameId = ref<string>();
const uninstallTarget = ref<InstalledEmulator>();
const confirmOpen = ref(false);
const resuming = ref<Record<string, boolean>>({});
const opening = ref<Record<string, boolean>>({});
const detailsPanel = ref<HTMLElement>();
const failedIcons = ref<Record<string, boolean>>({});
const failedBanners = ref<Record<string, boolean>>({});
const library = computed(() => emulatorLibrary(emulators.value));
const filteredLibrary = computed(() =>
  filterEmulatorLibrary(library.value, searchQuery.value),
);
const activeEmulator = computed(() =>
  emulatorSelection(library.value, selectedGameId.value),
);
watch(activeEmulator, (entry) => {
  selectedGameId.value = entry?.gameId;
});

function selectEmulator(gameId: string) {
  selectedGameId.value = gameId;
  detailsPanel.value?.scrollTo({ top: 0 });
}

function dependentHeadingId(version: InstalledEmulator) {
  return `required-${encodeURIComponent(version.gameId)}-${encodeURIComponent(version.versionId)}-${version.platform}`;
}

function requestUninstall(version: InstalledEmulator) {
  if (version.busy) return;
  uninstallTarget.value = version;
  confirmOpen.value = true;
}

async function resumeVersion(version: InstalledEmulator) {
  const key = dependentHeadingId(version);
  if (version.busy || resuming.value[key]) return;
  resuming.value[key] = true;
  error.value = "";
  try {
    await invoke("resume_download", {
      gameId: version.gameId,
      versionId: version.versionId,
      targetPlatform: version.platform,
    });
    await refresh();
  } catch (cause) {
    error.value = `Could not resume this emulator build: ${cause}`;
  } finally {
    resuming.value[key] = false;
  }
}

function canOpenEmulator(version: InstalledEmulator) {
  return emulatorCanOpen(version, opening.value[dependentHeadingId(version)]);
}

async function openEmulator(version: InstalledEmulator, launchId: string) {
  if (!canOpenEmulator(version)) return;
  const key = dependentHeadingId(version);
  opening.value[key] = true;
  error.value = "";
  try {
    await invoke("open_emulator", {
      gameId: version.gameId,
      versionId: version.versionId,
      targetPlatform: version.platform,
      launchId,
    });
    await refresh();
  } catch (cause) {
    error.value = `Could not open the emulator: ${cause}`;
  } finally {
    opening.value[key] = false;
  }
}

let refreshId = 0;
async function refresh() {
  const requestId = ++refreshId;
  loading.value = true;
  error.value = "";
  try {
    const installed = await invoke<InstalledEmulator[]>(
      "fetch_installed_emulators",
    );
    if (!disposed && requestId === refreshId) emulators.value = installed;
  } catch (cause) {
    if (!disposed && requestId === refreshId)
      error.value = `Could not load installed emulators: ${cause}. Try refreshing.`;
  } finally {
    if (!disposed && requestId === refreshId) loading.value = false;
  }
}

async function openFolder(emulator: InstalledEmulator) {
  try {
    await invoke("open_fs", { path: emulator.installDir });
  } catch (cause) {
    error.value = `Could not open the emulator folder: ${cause}`;
  }
}

let unlisten: UnlistenFn | undefined;
let disposed = false;
onMounted(async () => {
  let watchError = "";
  try {
    const stop = await listen("update_library", refresh);
    if (disposed) {
      stop();
      return;
    }
    unlisten = stop;
  } catch (cause) {
    watchError = `Could not watch emulator changes: ${cause}. Use Refresh to update this list.`;
  }
  if (!disposed) {
    await refresh();
    if (watchError && !error.value) error.value = watchError;
  }
});
onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});
</script>

<style scoped>
.emulator-button {
  @apply inline-flex min-h-10 items-center justify-center rounded-md bg-zinc-700/50 px-3 py-2 text-sm font-semibold text-zinc-200 transition-colors hover:bg-zinc-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-zinc-800;
}
</style>
