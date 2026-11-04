import { test } from "node:test";
import assert from "node:assert/strict";
import { requestPreferredSteamLaunch } from "../utils/steam-launch.ts";

test("explicit direct launch never asks Steam", async () => {
  let calls = 0;
  assert.equal(
    await requestPreferredSteamLaunch("game", true, async () => {
      calls++;
      return true;
    }),
    false,
  );
  assert.equal(calls, 0);
});
test("successful preferred launch returns a handoff, not a running state", async () => {
  assert.equal(
    await requestPreferredSteamLaunch("game", false, async (id) => {
      assert.equal(id, "game");
      return true;
    }),
    true,
  );
});
test("normal Drop launching remains available when Steam is not preferred", async () => {
  assert.equal(
    await requestPreferredSteamLaunch("game", false, async () => false),
    false,
  );
});
test("Steam errors propagate without silent direct-launch fallback", async () => {
  await assert.rejects(
    requestPreferredSteamLaunch("game", false, async () => {
      throw new Error("Switch Steam account");
    }),
    /Switch Steam account/,
  );
});
