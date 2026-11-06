<template>
  <div v-if="profile" class="my-4 space-y-4 border-t border-zinc-800 pt-4">
    <label class="block text-sm font-medium text-zinc-100">
      {{ $t("emulators.launchMode") }}
      <select
        v-model="profile.mode"
        class="mt-2 block w-full rounded-md border-zinc-700 bg-zinc-950 px-3 py-2 text-zinc-100 focus:border-blue-500 focus:ring-blue-500"
      >
        <option value="cli">{{ $t("emulators.cli") }}</option>
        <option value="ui">{{ $t("emulators.ui") }}</option>
      </select>
    </label>
    <p class="text-xs leading-relaxed text-zinc-400">
      {{ $t("emulators.profileHelp") }}
    </p>
    <code class="block break-words text-xs text-blue-300" v-text="tokens" />
    <h3 class="text-sm font-medium text-zinc-100">
      {{ $t("emulators.configFiles") }}
    </h3>
    <p class="text-xs text-zinc-400">{{ $t("emulators.configHelp") }}</p>
    <div
      v-for="(file, index) in profile.configFiles"
      :key="index"
      class="space-y-3 rounded-lg bg-zinc-950 p-3"
    >
      <label class="block text-xs text-zinc-400"
        >{{ $t("emulators.configPath") }}
        <input
          v-model="file.path"
          type="text"
          placeholder="settings.json"
          class="mt-1 block w-full rounded-md border-zinc-700 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100 focus:ring-blue-500"
        />
      </label>
      <label class="block text-xs text-zinc-400"
        >{{ $t("emulators.configContent") }}
        <textarea
          v-model="file.content"
          rows="6"
          spellcheck="false"
          class="mt-1 block w-full rounded-md border-zinc-700 bg-zinc-900 px-3 py-2 font-mono text-xs text-zinc-100 focus:ring-blue-500"
        />
      </label>
      <button
        type="button"
        class="min-h-10 text-sm text-red-400 hover:text-red-300"
        @click="profile.configFiles.splice(index, 1)"
      >
        {{ $t("emulators.removeConfig") }}
      </button>
    </div>
    <button
      type="button"
      :disabled="profile.configFiles.length >= 32"
      class="min-h-10 rounded-md bg-zinc-800 px-3 py-2 text-sm text-zinc-100 hover:bg-zinc-700 disabled:opacity-40"
      @click="profile.configFiles.push({ path: '', content: '' })"
    >
      {{ $t("emulators.addConfig") }}
    </button>
  </div>
</template>

<script setup lang="ts">
import type { EmulatorProfile } from "~/server/internal/library/emulator-profile";
const profile = defineModel<typeof EmulatorProfile.infer | undefined>({
  required: true,
});
const tokens =
  "{rom} · {gameDir} · {emulatorDir} · {configDir} · {gameDir:json}";
</script>
