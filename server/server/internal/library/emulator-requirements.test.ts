import assert from "node:assert/strict";
import test from "node:test";
import { emulatorRequirements } from "./emulator-requirements.ts";

const emulator = {
  gameVersion: {
    versionId: "emulator-v1",
    displayName: "1.2.3",
    versionPath: "emulator.zip",
    game: { id: "emulator", mName: "Example emulator" },
  },
};
const version = {
  versionId: "game-v1",
  displayName: "Game 1.0",
  versionPath: "game.zip",
  launches: [{ platform: "Linux", emulator }],
};

test("store requirements expose the exact emulator pin and game version", () => {
  assert.deepEqual(emulatorRequirements([version]), [
    {
      gameVersionId: "game-v1",
      gameVersionName: "Game 1.0",
      emulatorId: "emulator",
      emulatorName: "Example emulator",
      versionId: "emulator-v1",
      versionName: "1.2.3",
      platform: "Linux",
    },
  ]);
});

test("shared launch pins are deduplicated without merging platforms or game versions", () => {
  const result = emulatorRequirements([
    {
      ...version,
      launches: [
        ...version.launches,
        ...version.launches,
        { platform: "Windows", emulator },
      ],
    },
    { ...version, versionId: "game-v2" },
  ]);
  assert.equal(result.length, 3);
  assert.deepEqual(
    result.map((r) => [r.gameVersionId, r.platform]),
    [
      ["game-v1", "Linux"],
      ["game-v1", "Windows"],
      ["game-v2", "Linux"],
    ],
  );
});

test("different emulator versions remain distinct and names fall back to archive paths", () => {
  const older = {
    gameVersion: {
      ...emulator.gameVersion,
      versionId: "old",
      displayName: null,
    },
  };
  const result = emulatorRequirements([
    {
      ...version,
      displayName: null,
      launches: [...version.launches, { platform: "Linux", emulator: older }],
    },
  ]);
  assert.equal(result.length, 2);
  assert.equal(result[1]!.versionName, "emulator.zip");
  assert.equal(result[1]!.gameVersionName, "game.zip");
});

test("native games and games without published launches have no emulator requirements", () => {
  assert.deepEqual(
    emulatorRequirements([
      { ...version, launches: [{ platform: "Linux", emulator: null }] },
    ]),
    [],
  );
  assert.deepEqual(emulatorRequirements([{ ...version, launches: [] }]), []);
});
