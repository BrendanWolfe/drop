<template>
  <div class="space-y-3">
    <div>
      <label
        :for="`${id}-select`"
        class="block text-sm/6 font-medium text-zinc-100"
      >
        {{ $t("emulators.consoleType") }}
      </label>
      <select
        :id="`${id}-select`"
        v-model="selected"
        :disabled="disabled || creating"
        class="mt-2 block min-h-11 w-full rounded-md border-zinc-700 bg-zinc-950 py-2 text-sm text-zinc-100 focus:border-blue-500 focus:ring-blue-500 disabled:opacity-50"
      >
        <option :value="null">{{ $t("emulators.consoleUnspecified") }}</option>
        <option
          v-for="consoleType in consoleTypes"
          :key="consoleType.id"
          :value="consoleType.id"
        >
          {{ consoleType.name }}
        </option>
      </select>
    </div>
    <div>
      <label :for="`${id}-name`" class="block text-sm/6 text-zinc-400">
        {{ $t("emulators.newConsoleType") }}
      </label>
      <div class="mt-2 flex gap-2">
        <input
          :id="`${id}-name`"
          v-model="newName"
          type="text"
          maxlength="80"
          :disabled="disabled || creating"
          :placeholder="$t('emulators.consolePlaceholder')"
          class="min-h-11 min-w-0 flex-1 rounded-md border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 focus:border-blue-500 focus:ring-blue-500 disabled:opacity-50"
          @keydown.enter.prevent="createConsole"
        />
        <LoadingButton
          type="button"
          class="min-h-11 shrink-0"
          :loading="creating"
          :disabled="disabled || !newName.trim() || creating"
          @click="createConsole"
        >
          {{ $t("emulators.addConsoleType") }}
        </LoadingButton>
      </div>
    </div>
    <p v-if="error" role="alert" class="text-sm text-red-400">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
import { FetchError } from "ofetch";

defineProps<{ disabled?: boolean }>();
const emit = defineEmits<{ busy: [value: boolean] }>();
const selected = defineModel<string | null>({ default: null });
const id = useId();
const { t } = useI18n();
type ConsoleOption = { id: string; name: string };
const consoleTypes = ref(
  await $dropFetch<ConsoleOption[]>("/api/v1/admin/console-types"),
);
const newName = ref("");
const creating = ref(false);
const error = ref("");

async function createConsole() {
  if (!newName.value.trim() || creating.value) return;
  creating.value = true;
  emit("busy", true);
  error.value = "";
  try {
    const consoleType = await $dropFetch<ConsoleOption>(
      "/api/v1/admin/console-types",
      {
        method: "POST",
        body: { name: newName.value },
      },
    );
    if (!consoleTypes.value.some((entry) => entry.id === consoleType.id)) {
      consoleTypes.value.push(consoleType);
      consoleTypes.value.sort((a, b) => a.name.localeCompare(b.name));
    }
    selected.value = consoleType.id;
    newName.value = "";
  } catch (cause) {
    error.value =
      cause instanceof FetchError
        ? (cause.data?.message ?? t("emulators.consoleCreateFailed"))
        : t("emulators.consoleCreateFailed");
  } finally {
    creating.value = false;
    emit("busy", false);
  }
}
</script>
