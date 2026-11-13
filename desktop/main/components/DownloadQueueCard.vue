<template>
  <article
    class="flex flex-wrap items-center gap-4 rounded-lg bg-zinc-900 p-4"
    :data-download-key="downloadKey(item.meta)"
  >
    <button
      type="button"
      class="download-handle flex min-h-10 min-w-10 cursor-grab items-center justify-center rounded text-zinc-500 hover:bg-zinc-800 hover:text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 active:cursor-grabbing"
      :aria-label="`Drag ${title} to reorder`"
      :disabled="disabled"
    >
      <Bars3Icon class="size-5" aria-hidden="true" />
    </button>
    <img
      v-if="cover"
      :src="cover"
      alt=""
      class="h-20 w-16 shrink-0 rounded object-cover"
      draggable="false"
    />
    <div class="min-w-0 flex-1 basis-40">
      <NuxtLink
        :href="href"
        class="break-words font-display text-lg font-semibold text-zinc-100 hover:text-white"
        >{{ title }}</NuxtLink
      >
      <p class="mt-1 text-xs text-zinc-400">
        {{ item.meta.version }} · {{ item.meta.targetPlatform
        }}<span v-if="item.meta.downloadType === 'Emulator'"> · Emulator</span>
      </p>
      <p v-if="metadataError" class="mt-1 text-xs text-amber-200">
        Details unavailable. Your download remains queued.
      </p>
      <p
        class="mt-2 text-sm"
        :class="active ? 'text-blue-300' : 'text-zinc-400'"
      >
        {{ active && paused ? "Paused" : item.status }}
      </p>
    </div>
    <div v-if="active" class="w-full space-y-2 sm:w-64">
      <div
        class="flex justify-between gap-3 text-xs tabular-nums text-zinc-300"
      >
        <span
          >{{ formatKilobytes(item.dl_current / 1000) }}B /
          {{ formatKilobytes(item.dl_max / 1000) }}B</span
        >
        <span>{{ Math.floor((item.dl_progress ?? 0) * 100) }}%</span>
      </div>
      <div
        role="progressbar"
        :aria-label="`${title} download progress`"
        :aria-valuenow="Math.floor((item.dl_progress ?? 0) * 100)"
        aria-valuemin="0"
        aria-valuemax="100"
        class="h-2 overflow-hidden rounded-full bg-zinc-800"
      >
        <div
          class="h-full bg-blue-500 transition-[width] duration-300 motion-reduce:transition-none"
          :style="{ width: `${Math.min(100, (item.dl_progress ?? 0) * 100)}%` }"
        />
      </div>
      <p
        v-if="item.disk_max !== item.dl_max"
        class="text-xs tabular-nums text-zinc-400"
      >
        Disk: {{ formatKilobytes(item.disk_current / 1000) }}B /
        {{ formatKilobytes(item.disk_max / 1000) }}B
      </p>
      <div
        v-if="item.disk_max !== item.dl_max"
        role="progressbar"
        :aria-label="`${title} disk progress`"
        :aria-valuenow="Math.floor((item.disk_progress ?? 0) * 100)"
        aria-valuemin="0"
        aria-valuemax="100"
        class="h-1.5 overflow-hidden rounded-full bg-zinc-800"
      >
        <div
          class="h-full bg-blue-400 transition-[width] duration-300 motion-reduce:transition-none"
          :style="{
            width: `${Math.min(100, (item.disk_progress ?? 0) * 100)}%`,
          }"
        />
      </div>
    </div>
    <div class="flex items-center gap-1">
      <template v-if="pending">
        <button
          type="button"
          :disabled="disabled"
          @click="$emit('enqueue')"
          class="flex size-10 items-center justify-center rounded text-zinc-300 transition-transform hover:bg-zinc-800 hover:text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 active:scale-[0.96] disabled:opacity-30"
          :aria-label="`Add ${title} to queue`"
          title="Add to queue"
        >
          <QueueListIcon class="size-5" aria-hidden="true" />
        </button>
        <button
          type="button"
          :disabled="disabled"
          @click="$emit('activate')"
          class="flex size-10 items-center justify-center rounded text-zinc-300 transition-transform hover:bg-zinc-800 hover:text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 active:scale-[0.96] disabled:opacity-30"
          :aria-label="`Download ${title} now`"
          title="Download now"
        >
          <PlayIcon class="size-5" aria-hidden="true" />
        </button>
      </template>
      <template v-else-if="!active">
        <button
          type="button"
          :disabled="disabled"
          @click="$emit('activate')"
          class="min-h-10 min-w-10 rounded text-zinc-300 hover:bg-zinc-800 hover:text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-30"
          :aria-label="`Download ${title} next`"
          title="Download now"
        >
          <PlayIcon class="mx-auto size-5" />
        </button>
        <button
          type="button"
          :disabled="disabled || first"
          @click="$emit('move', -1)"
          class="min-h-10 min-w-10 rounded text-zinc-400 hover:bg-zinc-800 hover:text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-30"
          :aria-label="`Move ${title} up`"
        >
          <ArrowUpIcon class="mx-auto size-4" />
        </button>
        <button
          type="button"
          :disabled="disabled || last"
          @click="$emit('move', 1)"
          class="min-h-10 min-w-10 rounded text-zinc-400 hover:bg-zinc-800 hover:text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-30"
          :aria-label="`Move ${title} down`"
        >
          <ArrowDownIcon class="mx-auto size-4" />
        </button>
      </template>
      <button
        v-if="!pending"
        type="button"
        :disabled="disabled"
        @click="$emit('cancel')"
        class="min-h-10 min-w-10 rounded text-zinc-400 hover:bg-red-500/10 hover:text-red-300 focus-visible:outline focus-visible:outline-2 focus-visible:outline-blue-400 disabled:opacity-30"
        :aria-label="`Cancel ${title} download`"
        title="Cancel download"
      >
        <XMarkIcon class="mx-auto size-5" />
      </button>
    </div>
  </article>
</template>

<script setup lang="ts">
import {
  ArrowDownIcon,
  ArrowUpIcon,
  Bars3Icon,
  PlayIcon,
  QueueListIcon,
  XMarkIcon,
} from "@heroicons/vue/20/solid";
import type { QueueState } from "~/composables/downloads";
import { downloadKey } from "~/utils/download-queue";

const props = defineProps<{
  item: QueueState["queue"][number];
  title: string;
  cover?: string;
  active?: boolean;
  paused?: boolean;
  pending?: boolean;
  metadataError?: boolean;
  disabled?: boolean;
  first?: boolean;
  last?: boolean;
}>();
defineEmits<{
  activate: [];
  enqueue: [];
  move: [direction: number];
  cancel: [];
}>();
const href = computed(() =>
  props.item.meta.downloadType === "Emulator"
    ? "/emulators"
    : props.item.meta.id.startsWith("local-")
      ? `/library/local/${props.item.meta.id}`
      : `/library/${props.item.meta.id}`,
);
</script>
