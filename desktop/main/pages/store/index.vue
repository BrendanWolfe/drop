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
    ref="storeFrame"
    :src="convertedStoreUrl"
    title="Drop Store"
    class="grow w-full h-full"
  />
</template>
<script setup lang="ts">
import { ArrowPathIcon } from "@heroicons/vue/24/outline";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { Game } from "~/types";

const route = useRoute();
const router = useRouter();
const storeFrame = ref<HTMLIFrameElement>();
let handlingDownload = false;
let libraryUpdates = Promise.resolve();
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

function onStoreMessage(event: MessageEvent) {
  const change = getStoreLibraryChange(event, storeFrame.value?.contentWindow);
  if (!change) return;

  const startDownload = change.download && !handlingDownload;
  if (startDownload) handlingDownload = true;

  // Serialize updates so a slower add refresh cannot overwrite a later removal.
  libraryUpdates = libraryUpdates.then(async () => {
    try {
      const result = await invoke<{ library: Game[] }>("fetch_library", {
        hardRefresh: true,
      });
      if (startDownload) {
        if (!result.library.some((game) => game.id === change.gameId)) {
          throw new Error("The game is no longer in your library.");
        }
        await router.push({
          path: `/library/${change.gameId}`,
          query: { download: "1" },
        });
      }
    } catch (cause) {
      createModal(
        ModalType.Notification,
        {
          title: startDownload
            ? "Couldn't start download"
            : "Couldn't refresh library",
          description: startDownload
            ? `The game was added to your library, but Drop couldn't start its download. Open the Library tab to retry: ${cause}`
            : `Your library changed, but Drop couldn't refresh its list: ${cause}`,
          buttonText: "Close",
        },
        (_event, close) => close(),
      );
    } finally {
      if (startDownload) handlingDownload = false;
    }
  });
}

onMounted(() => {
  window.addEventListener("message", onStoreMessage);
  prepareStore();
});
onBeforeUnmount(() => window.removeEventListener("message", onStoreMessage));
</script>
