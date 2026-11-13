<template>
  <fieldset
    class="ml-7 max-h-64 w-full space-y-1 overflow-y-auto overscroll-contain border-l border-zinc-700 pl-4 pr-2"
  >
    <legend class="sr-only">{{ $t("emulators.consoleType") }}</legend>
    <label
      v-for="consoleType in consoles"
      :key="consoleType.id"
      class="flex min-h-11 items-center gap-3 py-1 text-sm text-zinc-400 lg:min-h-10"
    >
      <input
        v-model="selection[consoleType.id]"
        type="checkbox"
        name="consoles[]"
        :value="consoleType.id"
        class="size-4 shrink-0 rounded-sm border-zinc-700 bg-zinc-800 text-blue-600 focus:ring-blue-500"
      />
      <span class="min-w-0 break-words">{{ consoleType.name }}</span>
    </label>
  </fieldset>
</template>

<script setup lang="ts">
defineProps<{ consoles: Array<{ id: string; name: string }> }>();
const selection = defineModel<Record<string, boolean | undefined>>({
  required: true,
});
</script>
