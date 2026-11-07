import assert from "node:assert/strict";
import test from "node:test";
import { emulatorRepairPlan } from "../utils/emulator-repair.ts";

const requirement = {
  gameId: "emulator",
  versionId: "required-old-version",
  platform: "Windows",
};

test("emulator repair downloads only the missing pinned builds, not the game or latest version", () => {
  assert.deepEqual(emulatorRepairPlan([requirement]), [requirement]);
});

test("multiple launch options needing the same emulator queue one repair", () => {
  assert.deepEqual(emulatorRepairPlan([requirement, requirement]), [
    requirement,
  ]);
});

test("multiple emulator dependencies are all retained", () => {
  const second = { ...requirement, gameId: "other-emulator" };
  assert.deepEqual(emulatorRepairPlan([requirement, second]), [
    requirement,
    second,
  ]);
});

test("conflicting versions or platforms fail before any download is queued", () => {
  assert.throws(
    () =>
      emulatorRepairPlan([requirement, { ...requirement, versionId: "other" }]),
    /conflicting/,
  );
  assert.throws(
    () =>
      emulatorRepairPlan([requirement, { ...requirement, platform: "Linux" }]),
    /conflicting/,
  );
});
