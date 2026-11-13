<template>
  <div class="max-w-lg space-y-3 rounded-lg border border-zinc-700 p-4">
    <ConsoleTypeSelector
      v-model="selection"
      :disabled="saving"
      @busy="creating = $event"
    />
    <LoadingButton
      type="button"
      class="min-h-11"
      :loading="saving"
      :disabled="saving || creating || selection === saved"
      @click="saveConsole"
    >
      {{ $t("common.save") }}
    </LoadingButton>
    <p v-if="error" role="alert" class="text-sm text-red-400">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
const props = defineProps<{ gameId: string }>();
const saved = defineModel<string | null>({ default: null });
const selection = ref(saved.value);
const saving = ref(false);
const creating = ref(false);
const error = ref("");
const { t } = useI18n();

async function saveConsole() {
  if (saving.value || creating.value) return;
  saving.value = true;
  error.value = "";
  try {
    const result = await $dropFetch("/api/v1/admin/game/:id/console", {
      method: "PATCH",
      params: { id: props.gameId },
      body: { consoleTypeId: selection.value },
    });
    saved.value = result.consoleTypeId;
  } catch {
    error.value = t("emulators.consoleSaveFailed");
  } finally {
    saving.value = false;
  }
}
</script>
