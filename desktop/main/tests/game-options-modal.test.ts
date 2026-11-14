import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import test from "node:test";
import ts from "typescript";
import { computed, nextTick, ref, watch } from "vue";
import type { GameVersion } from "../types.ts";

const source = readFileSync(
  new URL("../components/GameOptionsModal.vue", import.meta.url),
  "utf8",
);
const script = source
  .split('<script setup lang="ts">')[1]!
  .split("</script>")[0]!;
const setup = ts.transpileModule(script, {
  compilerOptions: {
    module: ts.ModuleKind.CommonJS,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText;

function gameVersion(platform = "Windows"): GameVersion {
  return {
    userConfiguration: {
      launchTemplate: "%command%",
      overrideProtonPath: "",
      overrideHandler: undefined,
      enableUpdates: true,
    },
    setups: [],
    launches: [{ platform }],
  };
}

async function createModal(version: GameVersion | null | undefined) {
  const currentVersion = ref(version);
  const open = ref(true);
  const calls: Array<{ command: string; args: any }> = [];
  const stops: Array<() => void> = [];
  // Run the actual setup script with Tauri and Nuxt globals stubbed, but real
  // Vue reactivity. No DOM or running desktop client is needed.
  const state = await runInNewContext(
    `(async () => {
      ${setup}
      return { configuration, hasWindows, protonEnabled, save, saveLoading };
    })()`,
    {
      exports: {},
      require: (id: string) => {
        if (id === "@tauri-apps/api/core")
          return {
            invoke: async (command: string, args: any) => {
              calls.push({ command, args });
            },
          };
        if (id === "@tauri-apps/plugin-os") return { platform: () => "linux" };
        return {};
      },
      ref,
      computed,
      watch: (...args: Parameters<typeof watch>) => {
        const stop = watch(...args);
        stops.push(stop);
        return stop;
      },
      defineModel: () => open,
      defineProps: () => ({ gameId: "game-id" }),
      useGame: async () => ({
        game: { id: "game-id" },
        version: currentVersion,
      }),
      useAppState: () => ref({ umuState: "Installed" }),
    },
  );
  return {
    ...state,
    currentVersion,
    open,
    calls,
    stop: () => stops.forEach((stop) => stop()),
  };
}

for (const missing of [null, undefined]) {
  test(`missing version (${missing}) does not throw or save empty defaults`, async (t) => {
    const modal = await createModal(missing);
    t.after(modal.stop);
    assert.equal(modal.configuration.value, undefined);
    assert.equal(modal.hasWindows.value, false);
    assert.equal(modal.protonEnabled.value, false);
    await modal.save();
    assert.equal(modal.calls.length, 0);
    assert.equal(modal.saveLoading.value, false);
    assert.equal(modal.open.value, true);
  });
}

test("configuration and platform support recover when version metadata arrives", async (t) => {
  const modal = await createModal(null);
  t.after(modal.stop);
  modal.currentVersion.value = gameVersion();
  await nextTick();
  assert.equal(modal.configuration.value.launchTemplate, "%command%");
  assert.equal(modal.protonEnabled.value, true);

  modal.currentVersion.value = gameVersion("Linux");
  await nextTick();
  assert.equal(modal.hasWindows.value, false);
  assert.equal(modal.protonEnabled.value, false);
});

test("editing stays local until the configuration is explicitly saved", async (t) => {
  const modal = await createModal(gameVersion());
  t.after(modal.stop);
  modal.configuration.value.launchTemplate = "custom %command%";
  assert.equal(
    modal.currentVersion.value!.userConfiguration.launchTemplate,
    "%command%",
  );
  await modal.save();
  assert.equal(modal.calls.length, 1);
  assert.equal(modal.calls[0]!.command, "update_game_configuration");
  assert.equal(modal.calls[0]!.args.gameId, "game-id");
  assert.equal(modal.calls[0]!.args.options.launchTemplate, "custom %command%");
  assert.equal(modal.open.value, false);
});

test("losing version metadata prevents saving even before the watcher runs", async (t) => {
  const modal = await createModal(gameVersion());
  t.after(modal.stop);
  modal.currentVersion.value = null;
  await modal.save();
  await nextTick();
  assert.equal(modal.calls.length, 0);
  assert.equal(modal.configuration.value, undefined);
});

test("the library only mounts options when opened for an installed game", () => {
  const page = readFileSync(
    new URL("../pages/library/[id]/index.vue", import.meta.url),
    "utf8",
  );
  assert.match(
    page,
    /<GameOptionsModal\s+v-if="\s*configureModalOpen &&\s*status.type === 'Installed'/,
  );
  assert.match(source, /v-if="configuration"/);
  assert.match(
    source,
    /:disabled="!configuration \|\| !game.version.value \|\| saveLoading"/,
  );
});
