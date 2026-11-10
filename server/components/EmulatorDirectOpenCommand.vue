<template>
  <div class="space-y-2 whitespace-normal">
    <label class="block text-sm font-medium text-zinc-100">
      {{ $t("emulators.directOpenCommand") }}
      <input
        v-model="command"
        type="text"
        :disabled="disabled"
        placeholder="emulator.exe --gui"
        spellcheck="false"
        class="mt-2 block w-full rounded-md border-zinc-700 bg-zinc-950 px-3 py-2 font-mono text-sm text-zinc-100 focus:border-blue-500 focus:ring-blue-500 disabled:opacity-50"
      />
    </label>
    <p class="max-w-lg text-xs leading-relaxed text-zinc-400">
      {{ $t("emulators.directOpenHelp") }}
    </p>
  </div>
</template>

<script setup lang="ts">
const command = defineModel<string | undefined>();
defineProps<{ disabled?: boolean }>();
</script>
