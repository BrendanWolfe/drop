import assert from "node:assert/strict";
import test from "node:test";
import {
  defaultInstallIndex,
  installPlanLocations,
} from "../utils/install-locations.ts";
import { gameInstallPlan } from "../utils/emulator-install.ts";

test("each location list preselects its own saved default", () => {
  assert.equal(
    defaultInstallIndex({
      directories: ["/games", "/fast-games"],
      defaultIndex: 1,
    }),
    1,
  );
  assert.equal(
    defaultInstallIndex({
      directories: ["/emulators", "/other-emulators"],
      defaultIndex: 0,
    }),
    0,
  );
});

test("missing or invalid defaults safely fall back to the first location", () => {
  for (const defaultIndex of [null, -1, 99, 0.5]) {
    assert.equal(
      defaultInstallIndex({ directories: ["/games"], defaultIndex }),
      0,
    );
  }
});

test("emulated game downloads route the game and pinned emulators to separate folders", () => {
  const game = { gameId: "game", versionId: "game-v1" };
  const emulator = { gameId: "emulator", versionId: "emulator-v1" };
  const plan = installPlanLocations(
    gameInstallPlan(game, [emulator], true),
    game.gameId,
    "/games",
    "/emulators",
  );
  assert.deepEqual(plan, [
    { ...emulator, enableUpdates: false, installPath: "/emulators" },
    { ...game, enableUpdates: true, installPath: "/games" },
  ]);
});

test("all destinations are validated before any downloads can be queued", () => {
  const plan = [{ gameId: "emulator" }, { gameId: "game" }];
  assert.throws(
    () => installPlanLocations(plan, "game", "/games", undefined),
    /emulator installation location/,
  );
  assert.throws(
    () => installPlanLocations(plan, "game", undefined, "/emulators"),
    /installation location/,
  );
});

test("native games do not require an emulator folder", () => {
  assert.deepEqual(
    installPlanLocations([{ gameId: "game" }], "game", "/games", undefined),
    [{ gameId: "game", installPath: "/games" }],
  );
});
