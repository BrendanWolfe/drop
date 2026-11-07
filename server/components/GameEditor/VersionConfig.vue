<template>
  <li class="p-3 bg-zinc-800 ring-1 ring-zinc-700 shadow rounded-lg space-y-2">
    <div class="flex justify-between">
      <h1
        v-if="!isSetup(props.config)"
        class="font-semibold text-zinc-300 text-md"
      >
        {{ props.config.name }}
      </h1>
      <span class="flex items-center">
        <component
          :is="PLATFORM_ICONS[props.config.platform]"
          alt=""
          class="size-5 flex-shrink-0 text-blue-600"
        />
        <span class="ml-2 block truncate text-zinc-100 text-sm font-bold">{{
          props.config.platform
        }}</span>
      </span>
    </div>
    <label
      v-if="props.config.platform === 'Windows'"
      class="flex min-h-10 cursor-pointer items-start gap-3 text-sm text-zinc-200"
    >
      <input
        type="checkbox"
        :checked="protonCompatible"
        :disabled="saving"
        class="mt-1 rounded border-zinc-700 bg-zinc-950 text-blue-600 focus:ring-blue-600 disabled:opacity-50"
        @change="saveCompatibility"
      />
      <span class="whitespace-normal">
        {{ $t("proton.allowLinux") }}
        <span class="block text-xs text-zinc-400">{{
          $t("proton.eligibilityHelp")
        }}</span>
      </span>
    </label>
    <p
      v-if="saveError"
      role="alert"
      class="text-sm whitespace-normal text-red-400"
    >
      {{ saveError }}
    </p>
    <div
      class="inline-flex gap-x-1 items-center bg-zinc-950 text-zinc-400 mono rounded-md p-2"
    >
      <p>{{ props.config.command }}</p>
    </div>
    <EmulatorWidget
      v-if="!isSetup(props.config) && props.config.emulator"
      :emulator="{
        launchId: props.config.emulator.launchId,
        gameName: props.config.emulator.gameVersion.game.mName,
        gameIcon: props.config.emulator.gameVersion.game.mIconObjectId,
        versionName: (props.config.emulator.gameVersion.displayName ??
          props.config.emulator.gameVersion.versionPath)!,
        launchName: props.config.emulator.name,
        platform: props.config.emulator.platform,
      }"
    />
  </li>
</template>

<script setup lang="ts">
import type { AdminFetchGameType } from "~/server/api/v1/admin/game/[id]/index.get";

const props = defineProps<{
  gameId: string;
  config:
    | AdminFetchGameType["versions"][number]["setups"][number]
    | AdminFetchGameType["versions"][number]["launches"][number];
}>();

const saving = ref(false);
const protonCompatible = ref(props.config.protonCompatible);
watch(
  () => props.config.protonCompatible,
  (value) => {
    protonCompatible.value = value;
  },
);
const saveError = ref<string>();
const { t } = useI18n();

async function saveCompatibility(event: Event) {
  const input = event.target as HTMLInputElement;
  protonCompatible.value = input.checked;
  saving.value = true;
  saveError.value = undefined;
  try {
    const result = await $dropFetch(
      "/api/v1/admin/game/:id/versions/:versionid/compatibility",
      {
        method: "PATCH",
        params: { id: props.gameId, versionid: props.config.versionId },
        body: {
          kind: isSetup(props.config) ? "setup" : "launch",
          configId: isSetup(props.config)
            ? props.config.setupId
            : props.config.launchId,
          protonCompatible: input.checked,
        },
      },
    );
    emit("compatibility", result.protonCompatible);
  } catch {
    protonCompatible.value = props.config.protonCompatible;
    saveError.value = t("proton.saveError");
  } finally {
    saving.value = false;
  }
}

const emit = defineEmits<{ compatibility: [value: boolean] }>();

function isSetup(
  v: typeof props.config,
): v is AdminFetchGameType["versions"][number]["setups"][number] {
  return Object.prototype.hasOwnProperty.call(v, "setupId");
}
</script>
