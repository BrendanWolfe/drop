<template>
  <div v-if="windows" class="inline-flex flex-wrap items-center gap-2">
    <button type="button" class="steam-button" @click="open">
      {{ context?.registration ? "Steam options…" : "Add to Steam…" }}
    </button>
    <button
      v-if="preferSteam"
      type="button"
      class="steam-button"
      @click="$emit('direct')"
    >
      Play directly with Drop
    </button>
    <span
      v-if="context?.registration && context.registration.state !== 'Ready'"
      class="max-w-sm text-xs text-amber-300"
      >{{ context.registration.state }}</span
    >
    <span v-if="message" role="status" class="max-w-sm text-xs text-zinc-300">{{
      message
    }}</span>
    <span
      v-if="error && !visible && !cleanupOpen"
      role="alert"
      class="max-w-sm text-xs text-red-300"
      >{{ error }}</span
    >
    <ModalTemplate v-model="visible" size-class="max-w-2xl">
      <template #default>
        <h2 class="text-xl font-display font-semibold text-zinc-100">
          {{ context?.registration ? "Steam shortcut" : "Add to Steam" }}
        </h2>
        <p class="mt-2 text-sm text-zinc-400">
          Steam launches the executable directly. Overlay and Steam Input
          compatibility vary by game. This does not add Steamworks features.
        </p>
        <div v-if="context" class="mt-5 space-y-4">
          <p v-if="!context.profiles.length" class="text-sm text-amber-300">
            No local Steam profiles found. Install Steam and sign in once, then
            retry.
          </p>
          <div>
            <label for="steam-name" class="field-label">Shortcut name</label>
            <input
              id="steam-name"
              v-model="name"
              maxlength="200"
              class="steam-input"
            />
          </div>
          <div>
            <label for="steam-profile" class="field-label">Steam profile</label>
            <select
              id="steam-profile"
              v-model="profile"
              class="steam-input"
              :disabled="!!context.registration"
            >
              <option value="" disabled>Choose an account…</option>
              <option
                v-for="entry in context.profiles"
                :key="entry.id"
                :value="entry.id"
              >
                {{ entry.name }} ({{ entry.accountId }}) — {{ entry.steamRoot }}
              </option>
            </select>
            <p v-if="context.registration" class="mt-1 text-xs text-zinc-500">
              Remove this shortcut before exporting to another account.
            </p>
          </div>
          <div>
            <label for="steam-launch" class="field-label">Launch option</label>
            <select id="steam-launch" v-model="launchId" class="steam-input">
              <option value="" disabled>Choose a launch option…</option>
              <option
                v-for="entry in context.options"
                :key="entry.id"
                :value="entry.id"
              >
                {{ entry.name }}
              </option>
            </select>
          </div>
          <p
            v-if="!selected?.launch"
            class="rounded-md bg-amber-500/10 p-3 text-sm text-amber-200"
          >
            {{
              selected?.reason ||
              "A fully installed Windows game with a direct .exe launch configuration is required."
            }}
          </p>
          <dl v-else class="space-y-3 rounded-lg bg-zinc-950/60 p-4 text-sm">
            <div>
              <dt class="text-zinc-500">Executable</dt>
              <dd class="break-all text-zinc-200">
                {{ selected.launch.executable }}
              </dd>
            </div>
            <div>
              <dt class="text-zinc-500">Working directory</dt>
              <dd class="break-all text-zinc-200">
                {{ selected.launch.workingDirectory }}
              </dd>
            </div>
            <div>
              <dt class="text-zinc-500">Arguments</dt>
              <dd class="break-all text-zinc-200">
                {{
                  selected.launch.arguments.length
                    ? JSON.stringify(selected.launch.arguments)
                    : "None"
                }}
              </dd>
            </div>
          </dl>
          <p
            v-if="context.running"
            class="rounded-md bg-amber-500/10 p-3 text-sm text-amber-200"
          >
            Close Steam completely before adding, updating, or removing this
            shortcut. Drop will preserve your existing shortcuts. Exit Steam
            from its tray menu, then retry.
          </p>
          <div v-if="context.registration" class="space-y-3">
            <p
              class="text-sm"
              :class="
                context.registration.state === 'Ready'
                  ? 'text-green-300'
                  : 'text-amber-300'
              "
            >
              {{ context.registration.state }}
            </p>
            <label class="flex items-center gap-3 text-sm text-zinc-200"
              ><input
                type="checkbox"
                :checked="preferSteam"
                :disabled="busy"
                @change="setPreference"
              />Always launch through Steam when clicking Play</label
            >
            <div class="flex flex-wrap gap-2">
              <button
                type="button"
                class="steam-button"
                :disabled="busy"
                @click="play"
              >
                Play through Steam
              </button>
              <button
                type="button"
                class="steam-button"
                :disabled="busy"
                @click="
                  $emit('direct');
                  visible = false;
                "
              >
                Play directly with Drop
              </button>
              <button
                type="button"
                class="steam-button text-red-300"
                :disabled="busy || context.running"
                @click="remove"
              >
                Remove from Steam
              </button>
            </div>
          </div>
          <p class="text-xs leading-relaxed text-zinc-500">
            Steam launches are not tracked by Drop: no Stop button, Drop
            playtime, or duplicate-launch prevention. Drop launch/exit hooks,
            including cloud-save synchronization if configured, do not run.
            Troubleshoot launch errors in Steam. Your Steam account must match
            this shortcut.
          </p>
        </div>
        <p v-if="error" role="alert" class="mt-4 text-sm text-red-300">
          {{ error }}
        </p>
        <p v-if="message" role="status" class="mt-4 text-sm text-green-300">
          {{ message }}
        </p>
      </template>
      <template #buttons>
        <button
          type="button"
          class="steam-button bg-blue-600 text-white"
          :disabled="busy || !selected?.launch || !profile || context?.running"
          @click="save"
        >
          {{
            busy
              ? "Working…"
              : context?.registration
                ? "Update / add again"
                : "Add to Steam"
          }}
        </button>
        <button
          type="button"
          class="steam-button"
          :disabled="busy"
          @click="refresh"
        >
          Retry / refresh
        </button>
        <button
          type="button"
          class="steam-button"
          :disabled="busy"
          @click="visible = false"
        >
          Close
        </button>
      </template>
    </ModalTemplate>
    <ModalTemplate v-model="cleanupOpen">
      <template #default>
        <h2 class="text-xl font-semibold text-zinc-100">
          Remove the Steam shortcut too?
        </h2>
        <p class="mt-3 text-sm text-zinc-400">
          You can uninstall now and leave shortcut cleanup for later. Steam must
          be closed to remove it. Deferred shortcuts appear in Settings → Steam.
        </p>
        <p v-if="error" role="alert" class="mt-3 text-sm text-red-300">
          {{ error }}
        </p>
      </template>
      <template #buttons>
        <button
          type="button"
          class="steam-button"
          :disabled="busy"
          @click="finishCleanup(true)"
        >
          Remove shortcut and continue
        </button>
        <button
          type="button"
          class="steam-button"
          :disabled="busy"
          @click="finishCleanup(false)"
        >
          Keep shortcut; continue
        </button>
        <button
          type="button"
          class="steam-button"
          :disabled="busy"
          @click="resolveCleanup(false)"
        >
          Cancel
        </button>
      </template>
    </ModalTemplate>
  </div>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { platform } from "@tauri-apps/plugin-os";
import type { SteamContext } from "~/types/steam";
const props = defineProps<{ gameId: string; gameName: string }>();
defineEmits<{ direct: [] }>();
const windows = platform() === "windows";
const context = ref<SteamContext>();
const preferSteam = computed(
  () => context.value?.registration?.preferSteam ?? false,
);
const visible = ref(false);
const cleanupOpen = ref(false);
const busy = ref(false);
const error = ref("");
const message = ref("");
const name = ref(props.gameName);
const profile = ref("");
const launchId = ref("");
const selected = computed(() =>
  context.value?.options.find((option) => option.id === launchId.value),
);
async function refresh() {
  if (!windows) return;
  try {
    const next = await invoke<SteamContext>("steam_context", {
      id: props.gameId,
    });
    context.value = next;
    if (!profile.value)
      profile.value =
        next.registration?.profile ||
        next.selectedProfile ||
        (next.profiles.length === 1 ? next.profiles[0]!.id : "");
    if (!launchId.value)
      launchId.value =
        next.registration?.launchId ||
        (next.options.length === 1 ? next.options[0]!.id : "");
  } catch (e) {
    error.value = String(e);
  }
}
async function open() {
  error.value = "";
  message.value = "";
  await refresh();
  name.value = context.value?.registration?.name || props.gameName;
  visible.value = true;
}
async function perform(operation: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  message.value = "";
  try {
    await operation();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
async function save() {
  await perform(async () => {
    await invoke("steam_save", {
      id: props.gameId,
      profileId: profile.value,
      launchId: launchId.value,
      name: name.value,
    });
    await refresh();
    message.value =
      "Added to Steam. Start Steam to see the game in your library.";
  });
}
async function remove() {
  await perform(async () => {
    const registration = context.value?.registration;
    if (!registration) return;
    await invoke("steam_remove", { key: registration.key });
    await refresh();
    message.value = "Removed from Steam.";
  });
}
async function play() {
  await perform(async () => {
    await invoke("steam_play", { id: props.gameId });
    message.value =
      "Launch requested in Steam. This does not confirm that the game started.";
  });
}
async function setPreference(event: Event) {
  await perform(async () => {
    await invoke("steam_prefer", {
      id: props.gameId,
      prefer: (event.target as HTMLInputElement).checked,
    });
    await refresh();
  });
}
let cleanupResolve: ((result: boolean) => void) | undefined;
function resolveCleanup(result: boolean) {
  cleanupOpen.value = false;
  cleanupResolve?.(result);
  cleanupResolve = undefined;
}
async function beforeUninstall(): Promise<boolean> {
  if (!windows) return true;
  await refresh();
  if (!context.value?.registration) return true;
  error.value = "";
  cleanupOpen.value = true;
  return new Promise((resolve) => {
    cleanupResolve = resolve;
  });
}
async function finishCleanup(removeShortcut: boolean) {
  if (removeShortcut) {
    await remove();
    if (error.value) return;
  }
  resolveCleanup(true);
}
watch(cleanupOpen, (value) => {
  if (!value && cleanupResolve) resolveCleanup(false);
});
onMounted(refresh);
const unlisten = await listen("steam_changed", () => {
  void refresh();
});
const unlistenGame = await listen(`update_game/${props.gameId}`, () => {
  void refresh();
});
onBeforeUnmount(() => {
  unlisten();
  unlistenGame();
  resolveCleanup(false);
});
defineExpose({ preferSteam, play, beforeUninstall });
</script>
<style scoped>
.steam-button {
  @apply inline-flex min-h-10 items-center justify-center rounded-md bg-zinc-800 px-3 py-2 text-sm font-medium text-zinc-200 hover:bg-zinc-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:cursor-not-allowed disabled:opacity-40;
}
.steam-input {
  @apply block w-full rounded-md border-0 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 ring-1 ring-inset ring-zinc-700 focus:ring-2 focus:ring-blue-500;
}
.field-label {
  @apply mb-1.5 block text-sm font-medium text-zinc-200;
}
</style>
