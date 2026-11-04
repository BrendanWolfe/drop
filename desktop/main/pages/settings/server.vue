<template>
  <div class="border-b border-zinc-700 py-5">
    <h3 class="text-base font-semibold font-display leading-6 text-zinc-100">Server connection</h3>
  </div>
  <form class="mt-5 max-w-lg space-y-6" @submit.prevent="save">
    <div>
      <h4 class="text-sm font-medium text-zinc-100">Drop instance address</h4>
      <p class="mt-2 break-all text-sm text-zinc-400">{{ serverUrl }}</p>
    </div>
    <ProxyAuthFields v-model="proxyAuth" :disabled="loading" />
    <ProxyCleanupNotice :pending="cleanupPending" />
    <p v-if="error" role="alert" class="rounded-md bg-red-600/10 p-4 text-sm text-red-400">{{ error }}</p>
    <p v-if="saved" role="status" class="text-sm text-green-400">Connection settings saved.</p>
    <LoadingButton :loading="loading" :disabled="loading">Save connection settings</LoadingButton>
  </form>
</template>

<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { proxyAuthForm, proxyAuthPayload, type ConnectionSettings } from "~/utils/proxy-auth";

const settings = await invoke<ConnectionSettings>("fetch_connection_settings");
const serverUrl = settings.url;
const proxyAuth = ref(proxyAuthForm(settings.proxyAuth));
const cleanupPending = ref(settings.proxyAuth?.cleanupPending ?? false);
const loading = ref(false);
const error = ref<string>();
const saved = ref(false);

async function save() {
  loading.value = true;
  saved.value = false;
  error.value = undefined;
  try {
    await invoke("update_proxy_auth", { proxyAuth: proxyAuthPayload(proxyAuth.value) });
    const updated = await invoke<ConnectionSettings>("fetch_connection_settings");
    proxyAuth.value = proxyAuthForm(updated.proxyAuth);
    cleanupPending.value = updated.proxyAuth?.cleanupPending ?? false;
    saved.value = true;
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}
</script>
