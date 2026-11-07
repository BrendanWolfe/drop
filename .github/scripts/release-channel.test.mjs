import assert from "node:assert/strict";
import test from "node:test";
import { releaseChannel } from "./release-channel.mjs";

function channel(overrides = {}) {
  return releaseChannel({
    eventName: "push",
    ref: "refs/heads/develop",
    event: {},
    version: "0.4.5",
    runNumber: "42",
    ...overrides,
  });
}

test("develop publishes a distinct prerelease; main publishes a stable build", () => {
  assert.deepEqual(channel(), {
    branch: "develop",
    prerelease: true,
    tag: "v0.4.5-dev.42",
  });
  assert.deepEqual(channel({ ref: "refs/heads/main" }), {
    branch: "main",
    prerelease: false,
    tag: "v0.4.5+build.42",
  });
});

test("manual tags support version placeholders without changing the branch channel", () => {
  assert.equal(
    channel({
      eventName: "workflow_dispatch",
      ref: "refs/heads/main",
      event: { inputs: { tagName: "v__VERSION__" } },
    }).tag,
    "v0.4.5",
  );
  assert.equal(
    channel({
      eventName: "workflow_dispatch",
      event: { inputs: { tagName: "testing" } },
    }).prerelease,
    true,
  );
});

test("scheduled builds always use develop, regardless of the default branch", () => {
  assert.equal(
    channel({ eventName: "schedule", ref: "refs/heads/main" }).branch,
    "develop",
  );
});

test("published releases must match their target branch", () => {
  for (const [branch, prerelease] of [
    ["main", false],
    ["develop", true],
  ]) {
    assert.deepEqual(
      channel({
        eventName: "release",
        event: {
          release: { target_commitish: branch, prerelease, tag_name: "v0.4.5" },
        },
      }),
      { branch, prerelease, tag: "v0.4.5" },
    );
    assert.throws(
      () =>
        channel({
          eventName: "release",
          event: {
            release: {
              target_commitish: branch,
              prerelease: !prerelease,
              tag_name: "v0.4.5",
            },
          },
        }),
      /must be/,
    );
  }
});

test("feature branches, tag dispatches, and ambiguous release targets cannot publish", () => {
  assert.throws(
    () => channel({ ref: "refs/heads/feature" }),
    /main or develop/,
  );
  assert.throws(() => channel({ ref: "refs/tags/main" }), /main or develop/);
  assert.throws(
    () =>
      channel({
        eventName: "release",
        event: {
          release: {
            target_commitish: "abc123",
            prerelease: false,
            tag_name: "v0.4.5",
          },
        },
      }),
    /main or develop/,
  );
});

test("tags cannot inject workflow outputs or contain invalid git ref syntax", () => {
  for (const tagName of [
    "v1\nprerelease=false",
    "v1\rbranch=main",
    "bad tag",
    "../v1",
    "a//b",
    "bad.lock",
    "v1:bad",
    "-v1",
    "dev,value=latest",
    "dev,enable=true",
  ]) {
    assert.throws(
      () =>
        channel({
          eventName: "workflow_dispatch",
          event: { inputs: { tagName } },
        }),
      /valid, single-line/,
    );
  }
});
