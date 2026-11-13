<template>
  <div class="space-y-6">
    <header class="flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold text-zinc-100">
          {{ t("adminDownloads.title") }}
        </h1>
        <p class="mt-2 text-sm text-zinc-400">
          {{ t("adminDownloads.description") }}
        </p>
      </div>
      <div class="flex items-center gap-2 text-xs text-zinc-400" role="status">
        <span
          class="size-2 rounded-full"
          :class="connected ? 'bg-green-400' : 'bg-amber-400'"
        />
        {{
          accessDenied
            ? t("adminDownloads.accessDenied")
            : connected
              ? t("adminDownloads.live")
              : t("adminDownloads.reconnecting")
        }}
      </div>
    </header>

    <nav
      class="flex flex-wrap gap-2"
      :aria-label="t('clientActivity.navigation')"
    >
      <span
        class="rounded-md bg-blue-500/10 px-4 py-2 text-sm font-medium text-blue-200 ring-1 ring-inset ring-blue-400/20"
        >{{ t("clientActivity.live") }}</span
      >
      <NuxtLink
        href="/admin/download-history"
        class="rounded-md px-4 py-2 text-sm text-zinc-400 hover:bg-zinc-800 hover:text-white"
        >{{ t("clientActivity.historyTitle") }}</NuxtLink
      >
      <NuxtLink
        href="/admin/errors"
        class="rounded-md px-4 py-2 text-sm text-zinc-400 hover:bg-zinc-800 hover:text-white"
        >{{ t("clientActivity.errorsTitle") }}</NuxtLink
      >
    </nav>

    <section
      class="flex flex-wrap items-center gap-x-8 gap-y-4 rounded-xl border border-zinc-700/70 bg-zinc-950/40 px-5 py-4"
      :aria-label="t('adminDownloads.summary')"
    >
      <div>
        <span class="text-2xl font-semibold tabular-nums text-zinc-100">{{
          activeCount
        }}</span
        ><span class="ml-2 text-sm text-zinc-400">{{
          t("adminDownloads.downloadingCount")
        }}</span>
      </div>
      <div>
        <span class="text-2xl font-semibold tabular-nums text-zinc-100">{{
          t("adminDownloads.rate", { speed: bytes(totalSpeed) })
        }}</span
        ><span class="ml-2 text-sm text-zinc-400">{{
          t("adminDownloads.reportedSpeed")
        }}</span>
      </div>
      <label class="ml-auto flex items-center gap-3 text-sm text-zinc-400"
        >{{ t("adminDownloads.show") }}
        <select
          v-model="filter"
          class="rounded-md border-zinc-700 bg-zinc-800 py-1.5 text-sm text-zinc-200 focus:border-blue-500 focus:ring-blue-500"
        >
          <option value="all">{{ t("adminDownloads.all") }}</option>
          <option value="Downloading">
            {{ t("adminDownloads.states.Downloading") }}
          </option>
          <option value="Paused">
            {{ t("adminDownloads.states.Paused") }}
          </option>
          <option value="Queued">
            {{ t("adminDownloads.states.Queued") }}
          </option>
        </select>
      </label>
    </section>

    <p
      v-if="accessDenied"
      role="alert"
      class="rounded-lg border border-red-500/20 bg-red-500/10 p-4 text-sm text-red-300"
    >
      {{ t("adminDownloads.accessDeniedHelp") }}
    </p>

    <div v-if="filtered.length" class="grid grid-cols-1 gap-4 xl:grid-cols-2">
      <article
        v-for="download in filtered"
        :key="`${download.clientId}/${download.sessionId}`"
        class="overflow-hidden rounded-xl border border-zinc-700/70 bg-zinc-950/40"
      >
        <div class="space-y-5 p-5">
          <div class="flex items-start justify-between gap-4">
            <div class="min-w-0">
              <h2
                class="truncate text-lg font-semibold text-zinc-100"
                :title="download.gameTitle"
              >
                {{ download.gameTitle }}
              </h2>
              <p class="mt-1 truncate text-sm text-zinc-400">
                {{
                  t("adminDownloads.userDevice", {
                    user: download.userName,
                    device: download.deviceName,
                  })
                }}
              </p>
            </div>
            <span
              class="shrink-0 rounded-md border px-2 py-1 text-xs font-medium"
              :class="
                download.stale
                  ? 'border-amber-500/30 text-amber-300'
                  : download.state === 'Downloading'
                    ? 'border-blue-500/30 text-blue-300'
                    : 'border-zinc-700 text-zinc-400'
              "
              >{{
                download.stale
                  ? t("adminDownloads.disconnected")
                  : stateLabels[download.state]
              }}</span
            >
          </div>
          <div class="flex items-baseline justify-between gap-4 tabular-nums">
            <span class="text-3xl font-semibold tracking-tight text-zinc-100">{{
              download.bytesTotal
                ? `${percentage(download)}%`
                : t("adminDownloads.unknownProgress")
            }}</span>
            <span class="text-sm text-zinc-400">{{
              t("adminDownloads.byteProgress", {
                current: bytes(download.bytesDownloaded),
                total: download.bytesTotal
                  ? bytes(download.bytesTotal)
                  : t("adminDownloads.unknownSize"),
              })
            }}</span>
          </div>
          <div class="flex items-center justify-between gap-4 text-sm">
            <span class="font-medium tabular-nums text-zinc-200">{{
              t("adminDownloads.rate", {
                speed: bytes(connected ? download.bytesPerSecond : 0),
              })
            }}</span>
            <span class="text-xs text-zinc-500">{{
              download.stale ? t("adminDownloads.waiting") : eta(download)
            }}</span>
          </div>
        </div>
        <div
          class="h-1.5 bg-zinc-800"
          role="progressbar"
          :aria-label="
            t('adminDownloads.progress', { game: download.gameTitle })
          "
          :aria-valuenow="
            download.bytesTotal ? percentage(download) : undefined
          "
          aria-valuemin="0"
          aria-valuemax="100"
        >
          <div
            class="h-full transition-[width] duration-500 motion-reduce:transition-none"
            :class="
              download.stale
                ? 'bg-amber-500/70'
                : download.state === 'Downloading'
                  ? 'bg-blue-500'
                  : 'bg-zinc-500'
            "
            :style="{ width: `${percentage(download)}%` }"
          />
        </div>
      </article>
    </div>
    <div
      v-else
      class="rounded-xl border border-dashed border-zinc-700 px-6 py-16 text-center"
    >
      <ArrowDownTrayIcon
        class="mx-auto size-8 text-zinc-600"
        aria-hidden="true"
      />
      <h2 class="mt-4 text-base font-semibold text-zinc-200">
        {{
          filter === "all"
            ? t("adminDownloads.empty")
            : t("adminDownloads.noMatches")
        }}
      </h2>
      <p class="mt-2 text-sm text-zinc-500">
        {{
          filter === "all"
            ? t("adminDownloads.emptyHelp")
            : t("adminDownloads.filterHelp")
        }}
      </p>
    </div>
    <p class="text-xs text-zinc-500">
      {{ t("adminDownloads.privacy") }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { ArrowDownTrayIcon } from "@heroicons/vue/24/outline";
import type { ActiveDownload } from "~/server/internal/downloads/registry";
definePageMeta({ layout: "admin" });
const { t } = useI18n();
useHead(() => ({ title: t("adminDownloads.title") }));
const downloads = ref<ActiveDownload[]>(
  await $dropFetch<ActiveDownload[]>("/api/v1/admin/downloads"),
);
const connected = ref(false);
const accessDenied = ref(false);
const filter = ref("all");
const stateLabels = computed(() => ({
  Downloading: t("adminDownloads.states.Downloading"),
  Paused: t("adminDownloads.states.Paused"),
  Queued: t("adminDownloads.states.Queued"),
  Validating: t("adminDownloads.states.Validating"),
}));
const filtered = computed(() =>
  downloads.value.filter(
    (entry) => filter.value === "all" || entry.state === filter.value,
  ),
);
const activeCount = computed(() =>
  connected.value
    ? downloads.value.filter(
        (entry) => entry.state === "Downloading" && !entry.stale,
      ).length
    : 0,
);
const totalSpeed = computed(() =>
  connected.value
    ? downloads.value.reduce(
        (sum, entry) => sum + (entry.stale ? 0 : entry.bytesPerSecond),
        0,
      )
    : 0,
);
function bytes(value: number) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (value >= 1000 && i < units.length - 1) {
    value /= 1000;
    i++;
  }
  return `${value.toFixed(i ? 1 : 0)} ${units[i]}`;
}
function percentage(entry: ActiveDownload) {
  return entry.bytesTotal
    ? Math.min(
        100,
        Math.max(
          0,
          Math.floor((entry.bytesDownloaded / entry.bytesTotal) * 100),
        ),
      )
    : 0;
}
function eta(entry: ActiveDownload) {
  if (
    !connected.value ||
    entry.state !== "Downloading" ||
    !entry.bytesPerSecond ||
    !entry.bytesTotal
  )
    return "";
  const seconds = Math.max(
    0,
    Math.ceil(
      (entry.bytesTotal - entry.bytesDownloaded) / entry.bytesPerSecond,
    ),
  );
  return seconds < 60
    ? t("adminDownloads.secondsRemaining", { count: seconds })
    : seconds < 3600
      ? t("adminDownloads.minutesRemaining", { count: Math.ceil(seconds / 60) })
      : t("adminDownloads.hoursRemaining", {
          count: (seconds / 3600).toFixed(1),
        });
}
let socket: WebSocket | undefined;
let retry: ReturnType<typeof setTimeout> | undefined;
let stopped = false;
let delay = 1000;
function connect() {
  if (stopped || accessDenied.value) return;
  socket = new WebSocket(
    `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/api/v1/admin/downloads/ws`,
  );
  socket.onmessage = (event) => {
    if (event.data === "unauthenticated") {
      accessDenied.value = true;
      connected.value = false;
      downloads.value = [];
      socket?.close();
      return;
    }
    try {
      const snapshot: unknown = JSON.parse(event.data);
      if (Array.isArray(snapshot)) {
        downloads.value = snapshot as ActiveDownload[];
        connected.value = true;
        delay = 1000;
      }
    } catch {
      /* Wait for the next complete snapshot. */
    }
  };
  socket.onclose = (event) => {
    connected.value = false;
    if (event.code === 1008) {
      accessDenied.value = true;
      downloads.value = [];
    }
    if (!stopped && !accessDenied.value) {
      retry = setTimeout(connect, delay);
      delay = Math.min(delay * 2, 30_000);
    }
  };
  socket.onerror = () => {
    connected.value = false;
    socket?.close();
  };
}
onMounted(connect);
onBeforeUnmount(() => {
  stopped = true;
  clearTimeout(retry);
  socket?.close();
});
</script>
