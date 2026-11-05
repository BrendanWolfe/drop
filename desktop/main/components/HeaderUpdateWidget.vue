<script setup lang="ts">
import {
  ArrowPathIcon,
  ExclamationTriangleIcon,
} from "@heroicons/vue/20/solid";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  updateButtonLabel,
  type DesktopUpdateStatus,
} from "~/utils/desktop-update";

const status = ref<DesktopUpdateStatus>({
  phase: "idle",
  tag: null,
  downloaded: 0,
  total: null,
  error: null,
});
const busy = computed(() =>
  ["checking", "downloading", "installing"].includes(status.value.phase),
);
const visible = computed(
  () => !["idle", "disabled"].includes(status.value.phase),
);
const label = computed(() => updateButtonLabel(status.value));
const description = computed(() =>
  status.value.error
    ? `${status.value.error} Click to retry.`
    : status.value.phase === "ready"
      ? `Drop ${status.value.tag} has been downloaded and verified. Click to update and restart Drop.`
      : label.value,
);

let unlisten: UnlistenFn | undefined;
let disposed = false;
onMounted(async () => {
  try {
    // Listen first, then fetch a snapshot so downloads finished before mount are visible.
    let receivedEvent = false;
    const stop = await listen<DesktopUpdateStatus>(
      "desktop-update",
      (event) => {
        receivedEvent = true;
        status.value = event.payload;
      },
    );
    if (disposed) {
      stop();
      return;
    }
    unlisten = stop;
    const snapshot = await invoke<DesktopUpdateStatus>("desktop_update_status");
    if (!disposed && !receivedEvent) status.value = snapshot;
  } catch (error) {
    console.error("Could not read desktop update status", error);
  }
});
onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});

async function activate() {
  if (busy.value) return;
  try {
    await invoke(
      status.value.phase === "ready"
        ? "install_desktop_update"
        : "check_desktop_update",
    );
  } catch (error) {
    // Backend publishes the failure and preserves a verified download for install retries.
    console.error("Desktop update failed", error);
  }
}
</script>

<template>
  <div class="inline-flex items-center" role="status" aria-live="polite">
    <button
      v-if="visible"
      type="button"
      :disabled="busy"
      :aria-label="label"
      :title="description"
      :class="[
        'inline-flex min-h-10 items-center gap-2 rounded-sm px-3 text-sm font-medium transition-[background-color,color,transform] motion-safe:active:scale-[0.96] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-400 disabled:cursor-wait',
        status.phase === 'ready'
          ? 'bg-blue-600 text-white hover:bg-blue-500'
          : status.phase === 'error'
            ? 'bg-zinc-900 text-amber-400 hover:bg-zinc-800'
            : 'bg-zinc-900 text-zinc-400',
      ]"
      @click="activate"
    >
      <ExclamationTriangleIcon
        v-if="status.phase === 'error'"
        class="size-5 shrink-0"
        aria-hidden="true"
      />
      <ArrowPathIcon
        v-else
        :class="['size-5 shrink-0', busy && 'motion-safe:animate-spin']"
        aria-hidden="true"
      />
      <span class="tabular-nums">{{ label }}</span>
    </button>
  </div>
</template>
