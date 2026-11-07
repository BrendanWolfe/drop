import assert from "node:assert/strict";
import test from "node:test";
import {
  availableBuilds,
  buildLabel,
  findRequiredBuild,
  versionsForBuild,
} from "../utils/version-options.ts";

const options = [
  { platform: "Linux", runtime: "native" as const, versionId: "new" },
  { platform: "Windows", runtime: "proton" as const, versionId: "new" },
  { platform: "Windows", runtime: "proton" as const, versionId: "old" },
];

test("labels distinguish native Linux from Windows through Proton", () => {
  assert.equal(buildLabel(options[0]!), "Linux — Native");
  assert.equal(buildLabel(options[1]!), "Windows — Proton");
  assert.equal(
    buildLabel({ platform: "Windows", runtime: "native" }),
    "Windows — Native",
  );
});

test("emulator dependency recovery matches the exact version and build platform", () => {
  assert.equal(
    findRequiredBuild(options, { versionId: "new", platform: "Windows" }),
    options[1],
  );
  assert.equal(
    findRequiredBuild(options, { versionId: "missing", platform: "Windows" }),
    undefined,
  );
  assert.equal(
    findRequiredBuild(options, { versionId: "old", platform: "Linux" }),
    undefined,
  );
});

test("build selection is separate from version selection with one choice per platform", () => {
  assert.deepEqual(
    availableBuilds(options).map((option) => option.platform),
    ["Linux", "Windows"],
  );
  assert.deepEqual(
    versionsForBuild(options, "Windows").map((option) => option.versionId),
    ["new", "old"],
  );
  assert.deepEqual(
    versionsForBuild(options, "Linux").map((option) => option.versionId),
    ["new"],
  );
  assert.deepEqual(versionsForBuild(options, "macOS"), []);
});
