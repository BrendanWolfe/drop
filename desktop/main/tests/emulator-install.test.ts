import assert from "node:assert/strict";
import test from "node:test";
import { gameInstallPlan } from "../utils/emulator-install.ts";

const game = { gameId: "game", versionId: "game-version" };
const emulator = { gameId: "emulator", versionId: "emulator-version" };

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

test("legacy conflicting dependencies fail before changing an installation", () => {
  assert.throws(
    () =>
      gameInstallPlan(
        game,
        [emulator, { ...emulator, versionId: "different" }],
        true,
      ),
    /conflicting/,
  );
  assert.throws(() => gameInstallPlan(game, [game], true), /conflicting/);
});
