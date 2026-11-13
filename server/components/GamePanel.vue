<template>
  <NuxtLink
    v-if="game || defaultPlaceholder"
    :href="href"
    :class="{
      'title-on-hover': !fallbackArtwork,
      'transition-all duration-300 text-left hover:scale-[1.02] hover:shadow-lg hover:-translate-y-0.5':
        animate,
    }"
    class="group relative flex-1 min-w-42 max-w-48 h-64 rounded-lg overflow-hidden"
  >
    <div
      :class="{
        'transition-all duration-300 group-hover:scale-110': animate,
      }"
      class="absolute inset-0"
    >
      <img
        :src="imageProps.src"
        class="w-full h-full object-cover brightness-[90%]"
        :alt="imageProps.alt"
        loading="lazy"
        decoding="async"
        width="192"
        height="256"
        @error="coverFailed = true"
        @load="checkArtwork"
      />
      <div
        v-if="showTitleDescription"
        :class="{
          'transition-opacity duration-150 motion-reduce:transition-none':
            animate,
        }"
        class="tile-decoration absolute inset-0 bg-gradient-to-t from-zinc-950/80 via-zinc-950/0 to-transparent"
      />
    </div>

    <div
      v-if="showTitleDescription"
      :class="{
        'transition-opacity duration-150 motion-reduce:transition-none':
          animate,
      }"
      class="tile-decoration absolute bottom-0 left-0 w-full p-3"
    >
      <h1
        :class="{ 'group-hover:text-white transition-colors': animate }"
        class="text-zinc-100 text-sm font-bold font-display"
      >
        {{
          game ? game.mName : $t("settings.admin.store.dropGameNamePlaceholder")
        }}
      </h1>
    </div>
  </NuxtLink>
  <SkeletonCard
    v-else-if="defaultPlaceholder === false"
    :message="$t('store.noGame')"
  />
</template>

<script setup lang="ts">
import type { SerializeObject } from "nitropack";
import { isGeneratedCover } from "~/utils/game-panel-artwork";

const { t } = useI18n();
const {
  game,
  href = undefined,
  showTitleDescription = true,
  animate = true,
  defaultPlaceholder = false,
} = defineProps<{
  game:
    | SerializeObject<{
        id: string;
        mCoverObjectId: string;
        mName: string;
        isEmulated?: boolean;
      }>
    | undefined
    | null;
  href?: string;
  showTitleDescription?: boolean;
  animate?: boolean;
  defaultPlaceholder?: boolean;
}>();

const coverFailed = ref(false);
const generatedArtwork = ref(false);
const fallbackArtwork = computed(
  () => !game?.mCoverObjectId || coverFailed.value || generatedArtwork.value,
);
const imageProps = computed(() => ({
  src:
    game?.mCoverObjectId && !coverFailed.value
      ? useObject(game.mCoverObjectId)
      : "/game-panel-placeholder.png",
  alt: game?.mName ?? t("settings.admin.store.dropGameAltPlaceholder"),
}));

watch(
  () => game?.mCoverObjectId,
  () => {
    coverFailed.value = false;
    generatedArtwork.value = false;
  },
);

async function checkArtwork(event: Event) {
  const image = event.target as HTMLImageElement;
  if (coverFailed.value || !game?.mCoverObjectId) return;
  // All generated fallback images are 512px squares. Do not inspect ordinary covers.
  if (image.naturalWidth !== 512 || image.naturalHeight !== 512) return;
  const src = imageProps.value.src;
  const generated = await isGeneratedCover(src);
  if (src === imageProps.value.src) generatedArtwork.value = generated;
}
</script>

<style scoped>
/* Touch devices have no reliable hover: keep their game names readable. */
@media (hover: hover) {
  .title-on-hover .tile-decoration {
    opacity: 0;
  }

  .title-on-hover:hover .tile-decoration,
  .title-on-hover:focus-within .tile-decoration {
    opacity: 1;
  }
}

img.active {
  view-transition-name: selected-game;
  contain: layout;
}
</style>
