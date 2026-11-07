import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import {
  updateButtonLabel,
  updateTimestamp,
  type DesktopUpdateStatus,
} from "../utils/desktop-update.ts";

const status: DesktopUpdateStatus = {
  phase: "idle",
  tag: null,
  downloaded: 0,
  total: null,
  error: null,
};

test("update timestamps preserve exact instants and omit missing or invalid history", () => {
  const time = Date.parse("2026-11-07T12:34:00Z");
  assert.equal(updateTimestamp(time)?.iso, "2026-11-07T12:34:00.000Z");
  assert.ok(updateTimestamp(time)?.label);
  for (const invalid of [undefined, null, NaN, Infinity, -1, Number.MAX_VALUE])
    assert.equal(updateTimestamp(invalid), undefined);
});

test("the update widget sits immediately left of Downloads", () => {
  const header = readFileSync(
    new URL("../components/Header.vue", import.meta.url),
    "utf8",
  );
  assert.match(header, /<HeaderUpdateWidget\s*\/>\s*<HeaderQueueWidget/);
});

test("download progress handles unknown lengths and clamps percentages", () => {
  assert.equal(
    updateButtonLabel({ ...status, phase: "downloading" }),
    "Downloading update",
  );
  assert.equal(
    updateButtonLabel({ ...status, phase: "downloading", total: 0 }),
    "Downloading update",
  );
  assert.equal(
    updateButtonLabel({
      ...status,
      phase: "downloading",
      total: 200,
      downloaded: 99,
    }),
    "Downloading update 49%",
  );
  assert.equal(
    updateButtonLabel({
      ...status,
      phase: "downloading",
      total: 100,
      downloaded: 110,
    }),
    "Downloading update 100%",
  );
});

test("ready and failure states describe the action the button will perform", () => {
  assert.equal(
    updateButtonLabel({ ...status, phase: "ready" }),
    "Update and restart",
  );
  assert.equal(
    updateButtonLabel({
      ...status,
      phase: "ready",
      error: "Permission denied",
    }),
    "Retry update and restart",
  );
  assert.equal(
    updateButtonLabel({ ...status, phase: "error" }),
    "Retry update download",
  );
  assert.equal(
    updateButtonLabel({ ...status, phase: "installing" }),
    "Installing update",
  );
});
