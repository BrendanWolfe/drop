import assert from "node:assert/strict";
import test from "node:test";
import { validateEmulatorBindings } from "./emulator-bindings.ts";

const launch = {
  launchId: "play",
  platform: "Windows",
  protonCompatible: true,
  emulatorId: "old",
};
const emulator = {
  launchId: "new",
  platform: "Windows",
  protonCompatible: true,
  emulatorId: null,
  versionId: "v2",
  gameVersion: {
    gameId: "emulator",
    onlySetup: false,
    game: { type: "Emulator" },
  },
};
const binding = {
  launchId: "play",
  previousEmulatorId: "old",
  emulatorId: "new",
};

test("an emulator pin can change without modifying the game launch or version", () => {
  const original = structuredClone(launch);
  assert.equal(
    validateEmulatorBindings([launch], [binding], [emulator], true),
    undefined,
  );
  assert.deepEqual(launch, original);
});

test("rebinds reject wrong platforms, non-emulators, setup-only and nested emulator launches", () => {
  for (const invalid of [
    { ...emulator, platform: "Linux" },
    { ...emulator, emulatorId: "nested" },
    { ...emulator, gameVersion: { ...emulator.gameVersion, onlySetup: true } },
    {
      ...emulator,
      gameVersion: { ...emulator.gameVersion, game: { type: "Game" } },
    },
  ])
    assert.match(
      validateEmulatorBindings([launch], [binding], [invalid], true)!,
      /portable emulator/,
    );
});

test("emulated games cannot lose their emulator and Proton eligibility cannot disappear silently", () => {
  assert.match(
    validateEmulatorBindings(
      [launch],
      [{ ...binding, emulatorId: null }],
      [],
      true,
    )!,
    /every emulated game launch/,
  );
  assert.match(
    validateEmulatorBindings(
      [launch],
      [binding],
      [{ ...emulator, protonCompatible: false }],
      true,
    )!,
    /Proton/,
  );
});

test("all launches sharing an emulator build can be changed atomically", () => {
  const other = { ...launch, launchId: "multiplayer", emulatorId: "old-other" };
  const otherEmulator = { ...emulator, launchId: "new-other" };
  assert.equal(
    validateEmulatorBindings(
      [launch, other],
      [
        binding,
        {
          launchId: other.launchId,
          previousEmulatorId: "old-other",
          emulatorId: "new-other",
        },
      ],
      [emulator, otherEmulator],
      true,
    ),
    undefined,
  );
  assert.equal(
    validateEmulatorBindings(
      [launch, other],
      [
        binding,
        {
          launchId: other.launchId,
          previousEmulatorId: "old-other",
          emulatorId: "new-other",
        },
      ],
      [emulator, { ...otherEmulator, versionId: "v1" }],
      true,
    ),
    undefined,
  );
});

test("stale, missing, foreign and duplicated launch edits are rejected", () => {
  for (const bindings of [
    [],
    [binding, binding],
    [{ ...binding, launchId: "another-game" }],
    [{ ...binding, previousEmulatorId: "stale" }],
  ]) {
    assert.match(
      validateEmulatorBindings([launch], bindings, [emulator], true)!,
      /changed/,
    );
  }
  assert.match(
    validateEmulatorBindings([launch], [binding], [], true)!,
    /portable emulator/,
  );
});
