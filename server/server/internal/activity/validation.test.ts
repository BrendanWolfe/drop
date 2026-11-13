import assert from "node:assert/strict";
import test from "node:test";
import { ArkErrors } from "arktype";
import {
  DownloadActivityBody,
  ErrorReportBody,
  historyPage,
  redactReport,
  validTimestamp,
} from "./validation.ts";

const activity = {
  sessionId: "session",
  gameId: "game",
  versionId: "version",
  platform: "Linux",
  state: "Completed",
  occurredAt: 1000,
  bytesDownloaded: 1024,
  bytesTotal: 1024,
};
test("download history accepts explicit lifecycle outcomes, not inferred disappearance", () => {
  for (const state of [
    "Queued",
    "Downloading",
    "Completed",
    "Cancelled",
    "Failed",
  ])
    assert.ok(
      !(DownloadActivityBody({ ...activity, state }) instanceof ArkErrors),
    );
  for (const state of ["Empty", "Unknown", ""])
    assert.ok(
      DownloadActivityBody({ ...activity, state }) instanceof ArkErrors,
    );
  assert.ok(
    DownloadActivityBody({ ...activity, bytesDownloaded: -1 }) instanceof
      ArkErrors,
  );
  assert.ok(
    DownloadActivityBody({ ...activity, sessionId: "" }) instanceof ArkErrors,
  );
});
test("error reports are bounded and source-validated", () => {
  const report = {
    reportId: "report",
    source: "download",
    message: "Disk full",
    details: "",
    clientVersion: "0.4.11",
    occurredAt: 1000,
  };
  assert.ok(!(ErrorReportBody(report) instanceof ArkErrors));
  assert.ok(
    ErrorReportBody({ ...report, message: "x".repeat(2001) }) instanceof
      ArkErrors,
  );
  assert.ok(
    ErrorReportBody({ ...report, details: "x".repeat(12001) }) instanceof
      ArkErrors,
  );
  assert.ok(
    ErrorReportBody({ ...report, source: "unknown" }) instanceof ArkErrors,
  );
});
test("reports redact credentials and signed URL queries", () => {
  const text = redactReport(
    "Authorization: Bearer my-secret https://user:pass@host/game?token=signed password=hunter2 cookie=session123",
  );
  for (const secret of [
    "my-secret",
    "user:pass",
    "token=signed",
    "hunter2",
    "session123",
  ])
    assert.ok(!text.includes(secret), text);
});
test("pagination and timestamps reject unsafe input", () => {
  assert.deepEqual(historyPage({}), { skip: 0, take: 50, page: 1 });
  assert.deepEqual(historyPage({ page: "2" }), { skip: 50, take: 50, page: 2 });
  for (const page of ["0", "-1", "1.5", "Infinity", "100001", "bad"])
    assert.equal(historyPage({ page }), undefined);
  assert.ok(validTimestamp(Date.now()));
  assert.ok(!validTimestamp(Number.MAX_SAFE_INTEGER));
  assert.ok(!validTimestamp(Infinity));
  assert.ok(!validTimestamp(-1));
});
