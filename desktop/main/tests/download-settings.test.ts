import assert from "node:assert/strict";
import test from "node:test";
import {
  formatDownloadSpeedMegabytes,
  parseDownloadSpeedMegabytes,
} from "../utils/download-settings.ts";

test("decimal MB/s limits convert to bytes per second", () => {
  assert.deepEqual(parseDownloadSpeedMegabytes("0"), {
    valid: true,
    bytesPerSecond: 0,
  });
  assert.deepEqual(parseDownloadSpeedMegabytes("1"), {
    valid: true,
    bytesPerSecond: 1_000_000,
  });
  assert.deepEqual(parseDownloadSpeedMegabytes("1.5"), {
    valid: true,
    bytesPerSecond: 1_500_000,
  });
  assert.deepEqual(parseDownloadSpeedMegabytes(2.25), {
    valid: true,
    bytesPerSecond: 2_250_000,
  });
});

test("empty, negative, and non-finite limits are rejected", () => {
  for (const value of [
    "",
    " ",
    "-1",
    "not a number",
    "Infinity",
    Number.NaN,
    Number.POSITIVE_INFINITY,
    Number.NEGATIVE_INFINITY,
  ]) {
    assert.equal(parseDownloadSpeedMegabytes(value).valid, false);
  }
});

test("limits that exceed the safe integer range are rejected", () => {
  assert.equal(
    parseDownloadSpeedMegabytes(String(Number.MAX_SAFE_INTEGER)).valid,
    false,
  );
});

test("positive limits cannot round down to unlimited", () => {
  assert.equal(parseDownloadSpeedMegabytes("0.0000004").valid, false);
  assert.deepEqual(parseDownloadSpeedMegabytes("0.000001"), {
    valid: true,
    bytesPerSecond: 1,
  });
});

test("stored byte limits are displayed as decimal MB/s", () => {
  assert.equal(formatDownloadSpeedMegabytes(0), "0");
  assert.equal(formatDownloadSpeedMegabytes(1_000_000), "1");
  assert.equal(formatDownloadSpeedMegabytes(1_500_000), "1.5");
});
