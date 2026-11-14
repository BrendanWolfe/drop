<template>
  <form
    class="mt-8 max-w-2xl space-y-4 border-t border-zinc-700 pt-6"
    @submit.prevent="save"
  >
    <h2 class="text-xl font-semibold text-zinc-100">
      {{ $t("cloudSaves.storageTitle") }}
    </h2>
    <p class="text-sm text-zinc-400">{{ $t("cloudSaves.storageHelp") }}</p>
    <fieldset :disabled="loading || saving" class="grid gap-4 sm:grid-cols-2">
      <label class="text-sm text-zinc-200"
        >{{ $t("cloudSaves.archiveLimit") }}
        <input
          v-model.number="limits.sizeLimitMiB"
          type="number"
          min="1"
          max="512"
          step="1"
          required
          class="mt-2 block min-h-10 w-full rounded-md border-zinc-700 bg-zinc-800 text-zinc-100 focus:border-blue-500 focus:ring-blue-500"
        />
      </label>
      <label class="text-sm text-zinc-200"
        >{{ $t("cloudSaves.historyLimit") }}
        <input
          v-model.number="limits.history"
          type="number"
          min="1"
          max="20"
          step="1"
          required
          class="mt-2 block min-h-10 w-full rounded-md border-zinc-700 bg-zinc-800 text-zinc-100 focus:border-blue-500 focus:ring-blue-500"
        />
      </label>
    </fieldset>
    <LoadingButton
      type="submit"
      :disabled="loading || saving"
      :loading="saving"
      class="min-h-10"
      >{{ $t("common.save") }}</LoadingButton
    >
    <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
    <p v-if="saved" role="status" class="text-sm text-green-300">
      {{ $t("common.saved") }}
    </p>
  </form>
</template>

<script setup lang="ts">
type StorageLimits = { sizeLimitMiB: number; history: number };
const limits = ref<StorageLimits>({ sizeLimitMiB: 10, history: 3 });
const loading = ref(true);
const saving = ref(false);
const saved = ref(false);
const error = ref("");
const { t } = useI18n();
onMounted(async () => {
  try {
    limits.value = await $dropFetch<StorageLimits>(
      "/api/v1/admin/cloud-saves/settings",
    );
    loading.value = false;
  } catch {
    error.value = t("cloudSaves.saveFailed");
  }
});
async function save() {
  saving.value = true;
  saved.value = false;
  error.value = "";
  try {
    limits.value = await $dropFetch<StorageLimits>(
      "/api/v1/admin/cloud-saves/settings",
      {
        method: "PATCH",
        body: limits.value,
      },
    );
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
