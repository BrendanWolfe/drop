import { test } from "node:test";
import assert from "node:assert/strict";
import { DownloadRegistry, type ActiveDownload } from "./registry.ts";

const entry: Omit<ActiveDownload, "startedAt" | "updatedAt" | "stale"> = {
  sessionId: "session",
  gameId: "game",
  versionId: "version",
  state: "Downloading",
  bytesDownloaded: 50,
  bytesTotal: 100,
  bytesPerSecond: 25,
  clientId: "client",
  userId: "user",
  userName: "Player",
  deviceName: "Desktop",
  gameTitle: "Game",
};

test("separate devices downloading the same game get separate cards", () => {
  const registry = new DownloadRegistry();
  registry.replace("one", "boot", 1, [{ ...entry, clientId: "one" }], 0);
  registry.replace("two", "boot", 1, [{ ...entry, clientId: "two" }], 0);
  assert.equal(registry.snapshot(0).length, 2);
});

test("stale sessions stop contributing bandwidth and expire", () => {
  const registry = new DownloadRegistry();
  registry.replace("client", "boot", 1, [entry], 0);
  assert.equal(registry.snapshot(14_999)[0]!.stale, false);
  assert.equal(registry.snapshot(15_000)[0]!.bytesPerSecond, 0);
  assert.equal(registry.snapshot(15_000)[0]!.stale, true);
  assert.deepEqual(registry.snapshot(60_000), []);
});

test("empty snapshots clear finished downloads; old snapshots cannot resurrect them", () => {
  const registry = new DownloadRegistry();
  registry.replace("client", "boot", 1, [entry], 0);
  registry.replace("client", "boot", 2, [], 100);
  registry.replace("client", "boot", 1, [entry], 200);
  assert.deepEqual(registry.snapshot(200), []);
});

test("heartbeats preserve the start time and paused downloads report zero speed", () => {
  const registry = new DownloadRegistry();
  registry.replace("client", "boot", 1, [entry], 100);
  registry.replace("client", "boot", 2, [{ ...entry, state: "Paused" }], 1000);
  assert.equal(registry.snapshot(1000)[0]!.startedAt, 100);
  assert.equal(registry.snapshot(1000)[0]!.bytesPerSecond, 0);
});
