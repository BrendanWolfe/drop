<template>
  <NuxtLink
    v-if="onLinux"
    to="/settings/compat"
  >
    <HeaderWidget :problem="protonError">
      <img
        :src="protonLogoUrl"
        class="relative z-50 size-5 brightness-[30%]"
      />
    </HeaderWidget>
  </NuxtLink>
</template>

<script setup lang="ts">
const appState = useAppState();
const protonLogoUrl = `${useRuntimeConfig().app.baseURL.replace(/\/?$/, "/")}proton-logo.png`;
const onLinux = appState.value?.umuState !== "NotNeeded";
const paths = onLinux ? await useProtonPaths() : undefined;

const protonError = computed(
  () =>
    appState.value?.umuState === "NotInstalled" || !paths?.data.value.default,
);
</script>
