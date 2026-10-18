<template>
  <div
    v-if="loading || error !== null"
    class="grow w-full h-full flex items-center justify-center px-6 py-12"
  >
    <div class="max-w-lg text-center">
      <div v-if="loading" role="status">
        <ArrowPathIcon
          class="mx-auto h-12 w-12 animate-spin text-blue-600"
          aria-hidden="true"
        />
        <p class="mt-4 text-sm text-zinc-400">Preparing Store...</p>
      </div>
      <div v-else role="alert">
        <h1 class="text-3xl font-semibold font-display text-zinc-100">
          Unable to open Store
        </h1>
        <p class="mt-4 text-sm text-zinc-400">
          Store preparation failed. Check your connection and try again.
        </p>
        <p class="mt-2 break-words text-sm text-zinc-400">{{ error }}</p>
        <button
          type="button"
          class="mt-6 inline-flex items-center gap-x-2 text-sm font-semibold text-zinc-100"
          @click="prepareStore"
        >
          Retry <ArrowPathIcon class="h-5 w-5" aria-hidden="true" />
        </button>
      </div>
    </div>
  </div>
  <iframe
    v-else
    :src="convertedStoreUrl"
    title="Drop Store"
    class="grow w-full h-full"
  />
</template>
<script setup lang="ts">
import { ArrowPathIcon } from "@heroicons/vue/24/outline";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

const route = useRoute();
const loading = ref(true);
const error = ref<string | null>(null);

const convertedStoreUrl = computed(() => {
  const gameId = route.query.gameId;

  // This is necessary because convertFileSrc encodes the URI.
  return convertFileSrc("dummyvalue", "server").replace(
    "dummyvalue",
    typeof gameId === "string" && gameId
      ? `store/${encodeURIComponent(gameId)}`
      : "store",
  );
});

async function prepareStore() {
  loading.value = true;
  error.value = null;

  try {
    await invoke<void>("prepare_store");
  } catch (cause) {
    error.value =
      typeof cause === "string" && cause
        ? cause
        : cause instanceof Error
          ? cause.message
          : "An unexpected error occurred while preparing the Store.";
  } finally {
    loading.value = false;
  }
}

onMounted(prepareStore);
</script>
