import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import type { InstalledType, InstalledEmulator } from "../types.ts";
import {
  emulatorLibrary,
  emulatorCanOpen,
  emulatorSelection,
  emulatorVersionStatus,
  filterEmulatorLibrary,
} from "../utils/emulator-library.ts";

function installed(
  overrides: Partial<InstalledEmulator> = {},
): InstalledEmulator {
  return {
    gameId: "eden",
    name: "Eden",
    versionId: "eden-v1",
    versionName: "1.0",
    platform: "Linux",
    installDir: "/emulators/eden",
    installType: { type: "Installed" as InstalledType },
    busy: false,
    requiredBy: [
      { gameId: "mario", name: "Mario Kart World", versionName: "1.0" },
    ],
    ...overrides,
  };
}

test("the emulator sidebar groups installed builds by emulator and counts unique dependent games", () => {
  const versions = [
    installed({
      versionId: "eden-v2",
      versionName: "2.0",
      iconObjectId: "icon",
      bannerObjectId: "banner",
      shortDescription: "Switch emulator",
    }),
    installed({
      versionId: "eden-v10",
      versionName: "10.0",
      requiredBy: [
        { gameId: "mario", name: "Mario Kart World", versionName: "2.0" },
        { gameId: "zelda", name: "Zelda", versionName: "1.0" },
      ],
    }),
    installed({ gameId: "dolphin", name: "Dolphin", requiredBy: [] }),
  ];
  const entries = emulatorLibrary(versions);
  assert.deepEqual(
    entries.map((entry) => entry.name),
    ["Dolphin", "Eden"],
  );
  const eden = entries[1]!;
  assert.deepEqual(
    eden.versions.map((version) => version.versionName),
    ["10.0", "2.0"],
  );
  assert.equal(eden.requiredGameCount, 2);
  assert.equal(eden.iconObjectId, "icon");
  assert.equal(eden.bannerObjectId, "banner");
  assert.equal(eden.shortDescription, "Switch emulator");
  assert.equal(entries[0]!.requiredGameCount, 0);
  assert.equal(
    versions[0]!.versionName,
    "2.0",
    "grouping must not mutate the inventory",
  );
});

test("search ignores case and outer whitespace without changing the selected details", () => {
  const entries = emulatorLibrary([
    installed(),
    installed({ gameId: "dolphin", name: "Dolphin" }),
  ]);
  assert.deepEqual(
    filterEmulatorLibrary(entries, "  EDEN  ").map((entry) => entry.gameId),
    ["eden"],
  );
  assert.deepEqual(filterEmulatorLibrary(entries, "missing"), []);
  assert.equal(filterEmulatorLibrary(entries, " ").length, 2);
  assert.equal(emulatorSelection(entries, "dolphin")?.gameId, "dolphin");
});

test("selection survives inventory refreshes and falls back after the selected emulator is uninstalled", () => {
  const entries = emulatorLibrary([
    installed(),
    installed({ gameId: "dolphin", name: "Dolphin" }),
  ]);
  assert.equal(emulatorSelection(entries, undefined)?.gameId, "dolphin");
  assert.equal(
    emulatorSelection(emulatorLibrary([installed()]), "eden")?.gameId,
    "eden",
  );
  assert.equal(
    emulatorSelection(
      entries.filter((entry) => entry.gameId !== "eden"),
      "eden",
    )?.gameId,
    "dolphin",
  );
  assert.equal(emulatorSelection([], "eden"), undefined);
});

test("version status distinguishes installed, incomplete, setup-required, and busy builds", () => {
  assert.equal(emulatorVersionStatus(installed()), "Installed");
  assert.equal(
    emulatorVersionStatus(
      installed({
        installType: { type: "PartiallyInstalled" as InstalledType },
      }),
    ),
    "Installation incomplete",
  );
  assert.equal(
    emulatorVersionStatus(
      installed({ installType: { type: "SetupRequired" as InstalledType } }),
    ),
    "Setup required",
  );
  assert.equal(
    emulatorVersionStatus(
      installed({
        busy: true,
        installType: { type: "PartiallyInstalled" as InstalledType },
      }),
    ),
    "Operation in progress",
  );
});

test("version actions stay inline rather than detouring to an emulator game page", () => {
  const page = readFileSync(
    new URL("../pages/emulators.vue", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(page, /Manage version|\/library\/\$\{version\.gameId\}/);
  assert.match(page, /@click="openFolder\(version\)"/);
  assert.match(page, /@click="requestUninstall\(version\)"/);
  assert.match(page, /:disabled="version\.busy"/);
  assert.match(page, /invoke\("open_emulator"/);
  assert.match(page, /version\.directOpenLaunches\?\.length === 1/);
});

test("direct-open is available only for configured, complete, idle emulator versions", () => {
  const version = installed({
    directOpenLaunches: [{ launchId: "gui", name: "GUI" }],
  });
  assert.equal(emulatorCanOpen(version), true);
  assert.equal(emulatorCanOpen(installed()), false);
  assert.equal(emulatorCanOpen({ ...version, directOpenLaunches: [] }), false);
  assert.equal(emulatorCanOpen({ ...version, busy: true }), false);
  assert.equal(emulatorCanOpen(version, true), false);
  for (const type of [
    "PartiallyInstalled",
    "SetupRequired",
  ] as InstalledType[]) {
    assert.equal(emulatorCanOpen({ ...version, installType: { type } }), false);
  }
  const other = installed({ versionId: "v2", directOpenLaunches: [] });
  const versions = emulatorLibrary([version, other])[0]!.versions;
  assert.equal(
    versions.find((v) => v.versionId === version.versionId)!
      .directOpenLaunches![0]!.launchId,
    "gui",
  );
  assert.deepEqual(
    versions.find((v) => v.versionId === "v2")!.directOpenLaunches,
    [],
  );
});
