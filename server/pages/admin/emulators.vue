<template>
  <div class="space-y-6">
    <header class="flex flex-wrap items-center justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold text-zinc-100">
          {{ $t("emulators.title") }}
        </h1>
        <p class="mt-2 max-w-2xl text-sm text-zinc-400">
          {{ $t("emulators.description") }}
        </p>
      </div>
      <NuxtLink
        to="/admin/library/import?type=Emulator"
        class="rounded-md bg-blue-600 px-4 py-3 text-sm font-semibold text-white hover:bg-blue-500 focus-visible:outline-2 focus-visible:outline-blue-400"
        >{{ $t("emulators.add") }}</NuxtLink
      >
    </header>
    <aside class="rounded-xl bg-zinc-950/50 p-5 text-sm text-zinc-400">
      {{ $t("emulators.archiveHelp") }}
      <code class="mt-3 block break-all text-zinc-200">{{
        $t("emulators.archiveExample")
      }}</code>
    </aside>
    <label class="block max-w-lg text-sm text-zinc-300">
      {{ $t("library.search") }}
      <input
        v-model="query"
        type="search"
        class="mt-2 block w-full rounded-md border-zinc-700 bg-zinc-950 px-3 py-2 text-zinc-100 focus:border-blue-500 focus:ring-blue-500"
      />
    </label>
    <p v-if="error" role="alert" class="text-sm text-red-400">
      {{ $t("emulators.loadError") }}
    </p>
    <div
      v-else-if="data?.results.length"
      class="grid gap-4 md:grid-cols-2 xl:grid-cols-3"
      :aria-busy="status === 'pending'"
    >
      <NuxtLink
        v-for="emulator in data.results"
        :key="emulator.id"
        :to="`/admin/library/${emulator.id}`"
        class="rounded-xl bg-zinc-950/50 p-5 ring-1 ring-inset ring-zinc-800 transition-colors hover:bg-zinc-800/60 focus-visible:outline-2 focus-visible:outline-blue-400"
      >
        <div class="flex items-center gap-3">
          <img
            :src="useObject(emulator.mIconObjectId)"
            alt=""
            class="size-12 rounded-md outline outline-1 -outline-offset-1 outline-white/10"
          />
          <h2 class="font-semibold text-zinc-100">{{ emulator.mName }}</h2>
        </div>
        <p class="mt-4 text-sm text-zinc-300">
          {{
            emulator.versions[0]?.displayName ||
            emulator.versions[0]?.versionPath ||
            $t("emulators.noVersions")
          }}
        </p>
        <p class="mt-1 text-xs text-zinc-500">
          {{
            [
              ...new Set(
                emulator.versions.flatMap((v) =>
                  v.launches.map((l) => l.platform),
                ),
              ),
            ].join(" · ")
          }}
        </p>
        <p class="mt-4 text-sm text-blue-400">{{ $t("emulators.manage") }}</p>
      </NuxtLink>
    </div>
    <p
      v-else-if="status !== 'pending'"
      class="rounded-xl border border-dashed border-zinc-700 px-6 py-12 text-center text-sm text-zinc-400"
    >
      {{ $t("emulators.empty") }}
    </p>
    <nav
      v-if="data && data.count > 24"
      class="flex gap-3"
      :aria-label="$t('emulators.pagination')"
    >
      <button
        :disabled="skip === 0"
        class="rounded-md bg-zinc-800 px-4 py-2 text-sm text-white disabled:opacity-40"
        @click="skip -= 24"
      >
        {{ $t("emulators.previous") }}
      </button>
      <button
        :disabled="skip + 24 >= data.count"
        class="rounded-md bg-zinc-800 px-4 py-2 text-sm text-white disabled:opacity-40"
        @click="skip += 24"
      >
        {{ $t("emulators.next") }}
      </button>
    </nav>
  </div>
</template>

<script setup lang="ts">
import type fetchEmulators from "~/server/api/v1/admin/emulators/index.get";
type EmulatorList = Awaited<ReturnType<typeof fetchEmulators>>;
definePageMeta({ layout: "admin" });
const { t } = useI18n();
useHead(() => ({ title: t("emulators.title") }));
const query = ref("");
const skip = ref(0);
watch(query, () => {
  skip.value = 0;
});
const { data, status, error } = await useAsyncData(
  "admin-emulators",
  () =>
    $dropFetch<EmulatorList>("/api/v1/admin/emulators", {
      query: { query: query.value, skip: skip.value },
    }),
  { watch: [query, skip] },
);
</script>
