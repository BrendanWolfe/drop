<template>
  <fieldset :disabled="disabled" class="space-y-4">
    <legend class="text-sm font-semibold leading-6 text-zinc-100">
      Reverse Proxy Authentication
    </legend>
    <p :id="`${id}-description`" class="text-sm leading-6 text-zinc-400">
      Optional access tokens for a protected server. These are separate from your Drop login.
    </p>
    <label class="flex min-h-10 cursor-pointer items-center gap-3 text-sm text-zinc-100">
      <input
        v-model="model.enabled"
        type="checkbox"
        :aria-describedby="`${id}-description`"
        class="rounded border-zinc-600 bg-zinc-900 text-blue-600 focus:ring-blue-600"
      />
      Use custom authentication headers
    </label>
    <div v-if="model.enabled" class="space-y-4">
      <div v-for="(header, index) in model.headers" :key="index" class="space-y-2">
        <label :for="`${id}-name-${index}`" class="block text-sm font-medium text-zinc-100">
          Header name {{ index + 1 }}
        </label>
        <input
          :id="`${id}-name-${index}`"
          v-model="header.name"
          type="text"
          required
          autocomplete="off"
          autocapitalize="none"
          :spellcheck="false"
          class="block min-h-10 w-full rounded-md border-0 bg-zinc-900 px-3 py-2 text-sm text-zinc-100 ring-1 ring-inset ring-zinc-700 focus:ring-2 focus:ring-inset focus:ring-blue-600"
        />
        <label :for="`${id}-value-${index}`" class="block text-sm font-medium text-zinc-100">
          Header value {{ index + 1 }}
        </label>
        <input
          :id="`${id}-value-${index}`"
          v-model="header.value"
          type="password"
          :required="!hasSavedValue(header)"
          :placeholder="hasSavedValue(header) ? 'Saved token — leave blank to keep' : 'Enter token value'"
          autocomplete="new-password"
          autocapitalize="none"
          :spellcheck="false"
          class="block min-h-10 w-full rounded-md border-0 bg-zinc-900 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 ring-1 ring-inset ring-zinc-700 focus:ring-2 focus:ring-inset focus:ring-blue-600"
        />
      </div>
      <label class="flex min-h-10 cursor-pointer items-center gap-3 text-sm text-zinc-100">
        <input
          v-model="model.remember"
          type="checkbox"
          :aria-describedby="`${id}-storage`"
          class="rounded border-zinc-600 bg-zinc-900 text-blue-600 focus:ring-blue-600"
        />
        Remember tokens in OS credential storage
      </label>
      <p :id="`${id}-storage`" class="text-sm leading-6 text-zinc-400">
        Turn this off for session-only use if credential storage is unavailable.
        Tokens are sent only to this server's exact origin. Use HTTPS to protect them in transit.
      </p>
    </div>
  </fieldset>
</template>

<script setup lang="ts">
import { hasSavedValue, type ProxyAuthForm } from "~/utils/proxy-auth";

defineProps<{ disabled?: boolean }>();
const model = defineModel<ProxyAuthForm>({ required: true });
const id = useId();
</script>
