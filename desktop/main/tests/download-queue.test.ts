import assert from "node:assert/strict";
import test from "node:test";
import { downloadKey, queueMove } from "../utils/download-queue.ts";
import type { DownloadableMetadata } from "../types.ts";

const meta = (id: string, version = "v1"): DownloadableMetadata => ({
  id,
  version,
  targetPlatform: "Linux",
  downloadType: "Game" as DownloadableMetadata["downloadType"],
});
const queue = [meta("active"), meta("next"), meta("last")].map((meta) => ({
  meta,
}));

test("waiting-list moves offset indices without changing the active item", () => {
  const move = queueMove(queue, downloadKey(meta("last")), "waiting", 0)!;
  assert.equal(move.newIndex, 1);
  assert.equal(move.changesActive, false);
  assert.equal(move.meta.id, "last");
});
test("dropping into or out of the active slot requires confirmation", () => {
  assert.equal(
    queueMove(queue, downloadKey(meta("last")), "active", 1)?.changesActive,
    true,
  );
  assert.equal(
    queueMove(queue, downloadKey(meta("last")), "active", 1)?.newIndex,
    0,
  );
  assert.equal(
    queueMove(queue, downloadKey(meta("active")), "waiting", 0)?.changesActive,
    true,
  );
});
test("stale, invalid, and unchanged drag operations are ignored", () => {
  assert.equal(
    queueMove(queue, downloadKey(meta("missing")), "waiting", 0),
    undefined,
  );
  assert.equal(
    queueMove(queue, downloadKey(meta("next")), "waiting", 0),
    undefined,
  );
  assert.equal(
    queueMove(queue, downloadKey(meta("next")), "waiting", -1),
    undefined,
  );
});
test("build identity includes version, platform, and download type", () => {
  const base = meta("emulator");
  for (const other of [
    meta("emulator", "v2"),
    { ...base, targetPlatform: "Windows" },
    { ...base, downloadType: "Emulator" },
  ])
    assert.notEqual(
      downloadKey(base),
      downloadKey(other as DownloadableMetadata),
    );
});
