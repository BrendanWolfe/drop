// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: "2024-04-03",

  postcss: {
    plugins: {
      tailwindcss: {},
      autoprefixer: {},
    },
  },

  css: ["~/assets/main.scss"],

  ssr: false,
  devtools: false,

  extends: [["../../libraries/base"]],

  vite: {
    resolve: {
      // Shared-layer dialogs and desktop slots must use the same context keys.
      dedupe: ["vue", "@headlessui/vue"],
    },
  },

  app: {
    baseURL: "/main",
  },
});
