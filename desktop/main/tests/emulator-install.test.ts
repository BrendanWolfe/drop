import assert from "node:assert/strict";
import test from "node:test";
import {
  gameInstallPlan,
  emulatorRequirementsChanged,
} from "../utils/emulator-install.ts";

const game = { gameId: "game", versionId: "game-version" };
const emulator = { gameId: "emulator", versionId: "emulator-version" };

test("an exact installed emulator is reused without downloading it or requiring a destination", () => {
  assert.deepEqual(
    gameInstallPlan(
      game,
      [{ ...emulator, installedPath: "/existing/emulator" }],
      true,
    ),
    [{ ...game, enableUpdates: true }],
  );
});

test("mixed installed and missing dependencies only queue missing builds", () => {
  const missing = { gameId: "other-emulator", versionId: "v2" };
  assert.deepEqual(
    gameInstallPlan(
      game,
      [{ ...emulator, installedPath: "/existing/emulator" }, missing],
      false,
    ),
    [
      { ...missing, enableUpdates: false },
      { ...game, enableUpdates: false },
    ],
  );
});

test("readiness changes or administrator pin edits require review before installation", () => {
  const installed = {
    ...emulator,
    installedPath: "/emulator",
    launchIds: ["launch"],
  };
  assert.equal(emulatorRequirementsChanged([installed], [installed]), false);
  assert.equal(
    emulatorRequirementsChanged(
      [installed],
      [{ ...installed, installedPath: null }],
    ),
    true,
  );
  assert.equal(
    emulatorRequirementsChanged(
      [installed],
      [{ ...installed, versionId: "new" }],
    ),
    true,
  );
  assert.equal(
    emulatorRequirementsChanged(
      [installed],
      [{ ...installed, launchIds: ["other"] }],
    ),
    true,
  );
  assert.equal(
    emulatorRequirementsChanged(
      [emulator],
      [{ ...emulator, installedPath: "/now-installed" }],
    ),
    false,
  );
});

test("required emulators are installed first and never follow latest updates", () => {
  assert.deepEqual(gameInstallPlan(game, [emulator], true), [
    { ...emulator, enableUpdates: false },
    { ...game, enableUpdates: true },
  ]);
});

test("multiple launch options using one emulator do not queue it twice", () => {
  assert.equal(gameInstallPlan(game, [emulator, emulator], false).length, 2);
});

test("native game installation retains its existing update choice", () => {
  assert.deepEqual(gameInstallPlan(game, [], false), [
    { ...game, enableUpdates: false },
  ]);
});

test("different versions of the same emulator install side by side", () => {
  assert.deepEqual(
    gameInstallPlan(
      game,
      [emulator, { ...emulator, versionId: "different" }],
      true,
    ),
    [
      { ...emulator, enableUpdates: false },
      { ...emulator, versionId: "different", enableUpdates: false },
      { ...game, enableUpdates: true },
    ],
  );
  assert.throws(() => gameInstallPlan(game, [game], true), /conflicting/);
});
