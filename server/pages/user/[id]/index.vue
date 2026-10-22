<template>
  <div class="max-w-6xl mx-auto px-4 py-10">
    <div class="flex items-center gap-x-6">
      <img
        v-if="profile?.profilePictureObjectId"
        :src="useObject(profile.profilePictureObjectId)"
        class="w-24 h-24 rounded-md object-cover"
      />
      <div>
        <h1 class="text-2xl font-bold font-display text-zinc-100">
          {{ profile?.displayName ?? profile?.username ?? $t("user.unknown") }}
        </h1>
        <!-- eslint-disable-next-line @intlify/vue-i18n/no-raw-text -->
        <div v-if="profile" class="text-zinc-400 mt-1">@{{ profile.username }}</div>
        <p v-else class="text-zinc-400 mt-1">{{ $t("user.notFound") }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useObject } from "~/composables/objects";
import type { UserModel } from "~/prisma/client/models";

const route = useRoute();
const id = (route.params.id ?? "") as string;

let profile: UserModel | null = null;

try {
  profile = await $dropFetch(`/api/v1/user/${id}`);
} catch {
  profile = null;
}

useHead({
  title: profile?.displayName ?? profile?.username ?? "User",
});
</script>
