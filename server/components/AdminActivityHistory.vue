<template>
  <div class="space-y-6">
    <header>
      <h1 class="text-2xl font-semibold text-zinc-100">
        {{
          isErrors
            ? t("clientActivity.errorsTitle")
            : t("clientActivity.historyTitle")
        }}
      </h1>
      <p class="mt-2 text-sm text-zinc-400">
        {{
          isErrors
            ? t("clientActivity.errorsDescription")
            : t("clientActivity.historyDescription")
        }}
      </p>
    </header>
    <nav
      class="flex flex-wrap gap-2"
      :aria-label="t('clientActivity.navigation')"
    >
      <NuxtLink
        v-for="link in links"
        :key="link.href"
        :href="link.href"
        class="rounded-md px-4 py-2 text-sm font-medium focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400"
        :class="
          link.kind === kind
            ? 'bg-blue-500/10 text-blue-200 ring-1 ring-inset ring-blue-400/20'
            : 'text-zinc-400 hover:bg-zinc-800 hover:text-white'
        "
        >{{ link.label }}</NuxtLink
      >
    </nav>
    <form
      class="flex flex-wrap items-center justify-between gap-3"
      @submit.prevent="applySearch"
    >
      <label class="flex items-center gap-3 text-sm text-zinc-400"
        >{{ t("clientActivity.search") }}
        <input
          v-model="search"
          type="search"
          :placeholder="t('clientActivity.searchPlaceholder')"
          class="w-64 rounded-md border-zinc-700 bg-zinc-950 text-sm text-zinc-100 focus:border-blue-400 focus:ring-blue-400"
        />
      </label>
      <div class="flex gap-2">
        <button
          type="submit"
          class="min-h-10 rounded-md bg-zinc-800 px-4 py-2 text-sm text-zinc-100 hover:bg-zinc-700 disabled:opacity-40"
          :disabled="pending"
        >
          {{ t("clientActivity.search") }}
        </button>
        <button
          type="button"
          class="min-h-10 rounded-md bg-zinc-800 px-4 py-2 text-sm text-zinc-100 hover:bg-zinc-700 disabled:opacity-40"
          :disabled="pending"
          @click="refresh()"
        >
          {{ t("clientActivity.refresh") }}
        </button>
      </div>
    </form>
    <p
      v-if="error"
      role="alert"
      class="rounded-lg bg-red-500/10 p-4 text-sm text-red-200"
    >
      {{ t("clientActivity.loadError") }}
    </p>
    <p
      v-else-if="pending"
      role="status"
      class="py-12 text-center text-sm text-zinc-400"
    >
      {{ t("clientActivity.loading") }}
    </p>
    <template v-else>
      <div
        v-if="data?.items.length"
        class="overflow-x-auto rounded-xl bg-zinc-950/40 ring-1 ring-inset ring-white/10"
      >
        <table class="w-full text-left text-sm">
          <thead class="border-b border-zinc-700 text-xs text-zinc-400">
            <tr>
              <th class="px-5 py-4">{{ t("clientActivity.when") }}</th>
              <th class="px-5 py-4">{{ t("clientActivity.userDevice") }}</th>
              <th class="px-5 py-4">
                {{
                  isErrors
                    ? t("clientActivity.error")
                    : t("clientActivity.download")
                }}
              </th>
              <th class="px-5 py-4">
                {{
                  isErrors
                    ? t("clientActivity.source")
                    : t("clientActivity.outcome")
                }}
              </th>
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-800">
            <tr v-for="item in data.items" :key="item.id" class="align-top">
              <td
                class="whitespace-nowrap px-5 py-4 text-xs tabular-nums text-zinc-300"
              >
                <time :datetime="item.occurredAt ?? item.queuedAt">{{
                  date(item.occurredAt ?? item.queuedAt)
                }}</time>
                <template v-if="!isErrors"
                  ><p class="mt-2 text-zinc-500">
                    {{
                      t("clientActivity.started", {
                        time: date(item.startedAt),
                      })
                    }}
                  </p>
                  <p class="mt-1 text-zinc-500">
                    {{
                      t("clientActivity.ended", { time: date(item.endedAt) })
                    }}
                  </p></template
                >
              </td>
              <td class="px-5 py-4 text-zinc-200">
                <p>{{ item.userName }}</p>
                <p class="mt-1 text-xs text-zinc-500">{{ item.deviceName }}</p>
              </td>
              <td class="min-w-64 max-w-xl px-5 py-4">
                <template v-if="isErrors">
                  <p class="break-words text-red-200">{{ item.message }}</p>
                  <p v-if="item.gameTitle" class="mt-2 text-xs text-zinc-400">
                    {{ item.gameTitle }}
                  </p>
                  <details
                    v-if="item.details"
                    class="mt-3 text-xs text-zinc-400"
                  >
                    <summary class="cursor-pointer py-1 text-zinc-300">
                      {{ t("clientActivity.details") }}
                    </summary>
                    <pre
                      class="mt-2 max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-md bg-zinc-950 p-3 font-mono"
                      >{{ item.details }}</pre
                    >
                  </details>
                  <p class="mt-2 text-xs text-zinc-500">
                    {{ t("clientActivity.version") }} {{ item.clientVersion }}
                  </p>
                </template>
                <template v-else
                  ><p class="font-medium text-zinc-100">{{ item.gameTitle }}</p>
                  <p class="mt-1 text-xs text-zinc-500">
                    {{ `${item.versionId} · ${item.platform}` }}
                  </p>
                  <p class="mt-2 text-xs tabular-nums text-zinc-400">
                    {{
                      t("adminDownloads.byteProgress", {
                        current: bytes(item.bytesDownloaded),
                        total: bytes(item.bytesTotal),
                      })
                    }}
                  </p></template
                >
              </td>
              <td class="whitespace-nowrap px-5 py-4 text-xs">
                <span
                  :class="
                    item.state === 'Failed'
                      ? 'text-red-300'
                      : item.state === 'Completed'
                        ? 'text-green-300'
                        : 'text-zinc-300'
                  "
                  >{{ isErrors ? item.source : stateLabel(item.state) }}</span
                >
                <p v-if="!isErrors && !item.endedAt" class="mt-1 text-zinc-500">
                  {{ t("clientActivity.lastReported") }}
                </p>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <p
        v-else
        class="rounded-xl border border-dashed border-zinc-700 px-6 py-16 text-center text-sm text-zinc-400"
      >
        {{ t("clientActivity.empty") }}
      </p>
    </template>
    <div class="flex items-center justify-between gap-4">
      <button
        type="button"
        :disabled="page === 1 || pending"
        class="min-h-10 rounded-md bg-zinc-800 px-4 py-2 text-sm text-zinc-100 hover:bg-zinc-700 disabled:opacity-40"
        @click="page--"
      >
        {{ t("clientActivity.newer") }}
      </button>
      <span class="text-xs tabular-nums text-zinc-500">{{
        t("clientActivity.page", { page })
      }}</span>
      <button
        type="button"
        :disabled="!data?.hasMore || pending"
        class="min-h-10 rounded-md bg-zinc-800 px-4 py-2 text-sm text-zinc-100 hover:bg-zinc-700 disabled:opacity-40"
        @click="page++"
      >
        {{ t("clientActivity.older") }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
const { kind } = defineProps<{ kind: "downloads" | "errors" }>();
const { t } = useI18n();
const isErrors = computed(() => kind === "errors");
const page = ref(1);
const search = ref("");
const query = ref("");
const links = computed(() => [
  { href: "/admin/downloads", label: t("clientActivity.live"), kind: "live" },
  {
    href: "/admin/download-history",
    label: t("clientActivity.historyTitle"),
    kind: "downloads",
  },
  {
    href: "/admin/errors",
    label: t("clientActivity.errorsTitle"),
    kind: "errors",
  },
]);
interface ActivityRow {
  id: string;
  userName: string;
  deviceName: string;
  gameTitle?: string;
  queuedAt?: string;
  startedAt?: string | null;
  endedAt?: string | null;
  state?: string;
  versionId?: string;
  platform?: string;
  bytesDownloaded?: string;
  bytesTotal?: string;
  occurredAt?: string;
  message?: string;
  details?: string;
  source?: string;
  clientVersion?: string;
}
const { data, pending, error, refresh } = await useAsyncData(
  `activity-${kind}`,
  () =>
    $dropFetch<{ items: ActivityRow[]; hasMore: boolean }>(
      `${isErrors.value ? "/api/v1/admin/errors" : "/api/v1/admin/downloads/history"}?page=${page.value}&q=${encodeURIComponent(query.value)}`,
    ),
  { watch: [page, query] },
);
function applySearch() {
  page.value = 1;
  if (query.value === search.value) void refresh();
  else query.value = search.value;
}
function date(value?: string | null) {
  return value ? new Date(value).toLocaleString() : "—";
}
function bytes(value?: string) {
  let amount = Number(value ?? 0);
  let unit = 0;
  const units = ["B", "KB", "MB", "GB", "TB"];
  while (amount >= 1000 && unit < units.length - 1) {
    amount /= 1000;
    unit++;
  }
  return `${amount.toFixed(unit ? 1 : 0)} ${units[unit]}`;
}
function stateLabel(state?: string) {
  const labels: Record<string, string> = {
    Queued: t("adminDownloads.states.Queued"),
    Downloading: t("adminDownloads.states.Downloading"),
    Completed: t("clientActivity.completed"),
    Cancelled: t("clientActivity.cancelled"),
    Failed: t("clientActivity.failed"),
  };
  return labels[state ?? ""] ?? state;
}
</script>
