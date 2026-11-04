<template>
  <p v-if="pending" role="status" class="rounded-md bg-amber-500/10 p-4 text-sm leading-6 text-amber-300">
    Your connection settings are active, but old saved tokens could not yet be removed
    from OS credential storage. They will not be reused. Unlock credential storage and
    save again, or restart Drop, to retry cleanup.
  </p>
</template>

<script setup lang="ts">
defineProps<{ pending: boolean }>();
</script>
