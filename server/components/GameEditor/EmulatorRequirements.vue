<template>
  <ModalTemplate v-model="open" size-class="sm:max-w-2xl">
    <DialogTitle class="text-base font-semibold text-zinc-100">{{
      $t("emulators.editRequirements")
    }}</DialogTitle>
    <p class="text-sm whitespace-normal text-zinc-400">
      {{ $t("emulators.editRequirementsHelp") }}
    </p>
    <ul class="space-y-3">
      <li
        v-for="launch in launches"
        :key="launch.launchId"
        class="rounded-lg bg-zinc-950/50 p-3"
      >
        <h3 class="text-sm font-semibold text-zinc-100">
          {{ launch.name }}
          <span class="ml-2 text-zinc-400">{{ launch.platform }}</span>
        </h3>
        <EmulatorWidget
          v-if="launch.emulator"
          :emulator="launch.emulator"
          class="mt-2"
        />
        <p v-else class="mt-2 text-sm text-zinc-400">
          {{ $t("library.admin.launchRow.noEmulatorSelected") }}
        </p>
        <button
          type="button"
          :disabled="saving"
          class="mt-3 inline-flex min-h-10 items-center rounded-md bg-zinc-800 px-3 py-2 text-sm font-semibold text-zinc-100 hover:bg-zinc-700 focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-50"
          @click="
            selectedLaunch = launch.launchId;
            selectorOpen = true;
          "
        >
          {{ $t("library.admin.launchRow.emulatorSelect") }}
        </button>
      </li>
    </ul>
    <p v-if="error" role="alert" class="text-sm whitespace-normal text-red-300">
      {{ error }}
    </p>
    <template #buttons>
      <LoadingButton
        :loading="saving"
        :disabled="saving || !dirty"
        @click="save"
        >{{ $t("emulators.saveRequirements") }}</LoadingButton
      >
      <button
        type="button"
        :disabled="saving"
        class="inline-flex min-h-10 items-center rounded-md bg-zinc-800 px-3 py-2 text-sm font-semibold text-zinc-100 hover:bg-zinc-700 focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-50"
        @click="open = false"
      >
        {{ $t("cancel") }}
      </button>
    </template>
  </ModalTemplate>
  <ModalSelectLaunch
    v-if="activeLaunch"
    v-model="selectorOpen"
    :filter-platform="activeLaunch.platform"
    @select="selectEmulator"
  />
</template>

<script setup lang="ts">
import { DialogTitle } from "@headlessui/vue";
import type { SerializeObject } from "nitropack";
import type { EmulatorLaunchObject } from "~/composables/frontend";
import type { AdminFetchGameType } from "~/server/api/v1/admin/game/[id]/index.get";

type Version = SerializeObject<AdminFetchGameType>["versions"][number];
const open = defineModel<boolean>({ required: true });
const props = defineProps<{ gameId: string; version: Version }>();
const emit = defineEmits<{ saved: [launches: Version["launches"]] }>();
const launches = ref(
  props.version.launches.map((launch) => ({
    launchId: launch.launchId,
    name: launch.name,
    platform: launch.platform,
    previousEmulatorId: launch.emulatorId,
    emulator: launch.emulator
      ? ({
          launchId: launch.emulator.launchId,
          gameName: launch.emulator.gameVersion.game.mName,
          gameIcon: launch.emulator.gameVersion.game.mIconObjectId,
          versionName:
            launch.emulator.gameVersion.displayName ||
            launch.emulator.gameVersion.versionPath ||
            launch.emulator.versionId,
          launchName: launch.emulator.name,
          platform: launch.emulator.platform,
        } as EmulatorLaunchObject)
      : undefined,
  })),
);
const selectedLaunch = ref("");
const selectorOpen = ref(false);
const activeLaunch = computed(() =>
  launches.value.find((launch) => launch.launchId === selectedLaunch.value),
);
const dirty = computed(() =>
  launches.value.some(
    (launch) =>
      (launch.emulator?.launchId ?? null) !== launch.previousEmulatorId,
  ),
);
const saving = ref(false);
const error = ref("");

function selectEmulator(emulator: EmulatorLaunchObject) {
  if (activeLaunch.value) activeLaunch.value.emulator = emulator;
  error.value = "";
}
async function save() {
  if (saving.value || !dirty.value) return;
  saving.value = true;
  error.value = "";
  try {
    const result = await $dropFetch(
      "/api/v1/admin/game/:id/versions/:versionid/emulators",
      {
        method: "PATCH",
        params: { id: props.gameId, versionid: props.version.versionId },
        body: {
          launches: launches.value.map((launch) => ({
            launchId: launch.launchId,
            emulatorId: launch.emulator?.launchId ?? null,
            previousEmulatorId: launch.previousEmulatorId,
          })),
        },
      },
    );
    emit("saved", result.launches);
    open.value = false;
  } catch (cause) {
    const failure = cause as {
      data?: { message?: string; statusMessage?: string };
      message?: string;
    };
    error.value =
      failure.data?.message ||
      failure.data?.statusMessage ||
      failure.message ||
      String(cause);
  } finally {
    saving.value = false;
  }
}
</script>
