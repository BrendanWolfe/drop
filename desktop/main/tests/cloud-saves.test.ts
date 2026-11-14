import assert from "node:assert/strict";
import test from "node:test";
import {
  cloudActionNeedsChoice,
  snapshotLabel,
  type CloudSaveStatus,
  type CloudSnapshot,
} from "../utils/cloud-saves.ts";

test("background snapshots are never presented as confirmed closed", () => {
  const snapshot: CloudSnapshot = {
    id: "snapshot",
    revision: 2,
    definitionRevision: 1,
    clientId: "client",
    kind: "background",
    checksum: "",
    contentHash: "",
    bytes: 1,
    createdAt: "2026-11-14T00:00:00Z",
  };
  assert.match(
    snapshotLabel(snapshot),
    /Background recovery \(game may have been running\)/,
  );
  assert.match(
    snapshotLabel({ ...snapshot, kind: "manual" }),
    /Confirmed closed/,
  );
});

test("a local-over-cloud choice is explicit and never inferred from being offline", () => {
  assert.equal(cloudActionNeedsChoice(null), false);
  assert.equal(
    cloudActionNeedsChoice({ phase: "offline" } as CloudSaveStatus),
    false,
  );
  assert.equal(
    cloudActionNeedsChoice({ phase: "conflict" } as CloudSaveStatus),
    true,
  );
});
