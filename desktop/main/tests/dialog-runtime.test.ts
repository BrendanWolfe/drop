import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import test from "node:test";
import { createServer } from "vite";

const config = runInNewContext(
  readFileSync(new URL("../nuxt.config.ts", import.meta.url), "utf8").replace(
    "export default ",
    "",
  ),
  { defineNuxtConfig: (value: unknown) => value },
);

test("shared modal parents and desktop titles resolve the same dialog and Vue runtimes", async () => {
  const desktopRoot = fileURLToPath(new URL("../", import.meta.url));
  const server = await createServer({
    root: desktopRoot,
    configFile: false,
    server: { middlewareMode: true, watch: null },
    resolve: config.vite?.resolve,
  });
  try {
    const parent = fileURLToPath(
      new URL(
        "../../../libraries/base/components/ModalTemplate.vue",
        import.meta.url,
      ),
    );
    const children = [
      "EmulatorUninstallModal.vue",
      "EmulatorRepairModal.vue",
      "InstallLocationSettings.vue",
    ].map((name) =>
      fileURLToPath(new URL(`../components/${name}`, import.meta.url)),
    );
    for (const dependency of ["@headlessui/vue", "vue"]) {
      const sharedRuntime = await server.pluginContainer.resolveId(
        dependency,
        parent,
      );
      assert.ok(
        sharedRuntime,
        `Could not resolve ${dependency} for the shared modal`,
      );
      for (const child of children) {
        const desktopRuntime = await server.pluginContainer.resolveId(
          dependency,
          child,
        );
        assert.equal(
          desktopRuntime?.id,
          sharedRuntime.id,
          `${dependency} must share context between ${parent} and ${child}`,
        );
      }
    }
  } finally {
    await server.close();
  }
});
