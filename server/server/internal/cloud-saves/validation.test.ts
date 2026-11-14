import assert from "node:assert/strict";
import test from "node:test";
import {
  parseConfiguration,
  parseLocations,
  parseNamespace,
  parseStorageLimits,
  parseUploadHeaders,
} from "./validation.ts";

const directory = {
  id: "profiles",
  platform: "Windows",
  path: "<winLocalAppData>/ExampleGame/Saves",
  type: "directory",
  exclude: ["Cache/**", "**/*.lock"],
};

test("admin opt-in requires an explicit boolean and at least one save location", () => {
  assert.deepEqual(
    parseConfiguration({ enabled: false, locations: [], revision: 0 }),
    { enabled: false, locations: [], revision: 0 },
  );
  assert.equal(
    parseConfiguration({ enabled: true, locations: [directory], revision: 0 })
      .locations[0]?.type,
    "directory",
  );
  for (const body of [
    { locations: [] },
    { enabled: "true", locations: [directory], revision: 0 },
    { enabled: true, locations: [], revision: 0 },
    { enabled: false, locations: [], revision: -1 },
  ])
    assert.throws(() => parseConfiguration(body));
});

test("storage limits are bounded integers and do not enable games or devices", () => {
  assert.deepEqual(parseStorageLimits({ sizeLimitMiB: 512, history: 20 }), {
    sizeLimitMiB: 512,
    history: 20,
  });
  for (const sizeLimitMiB of [0, 513, -1, 1.5, "10", Number.NaN])
    assert.throws(() => parseStorageLimits({ sizeLimitMiB, history: 3 }));
  for (const history of [0, 21, 2.5, "3", null])
    assert.throws(() => parseStorageLimits({ sizeLimitMiB: 10, history }));
});

test("files and recursive directories retain stable IDs and relative exclusions", () => {
  const file = {
    id: "progress",
    platform: "Linux",
    path: "<xdgData>/ExampleGame/progress.dat",
    type: "file",
  };
  assert.deepEqual(parseLocations([directory, file])[1], {
    ...file,
    exclude: [],
  });
  assert.throws(() => parseLocations([directory, directory]));
});

test("unsafe, broad, absolute and cross-platform paths are rejected", () => {
  for (const path of [
    "<home>",
    "<home>/../.ssh",
    "<home>//save",
    "<home>/game/*",
    "<home>\\game\\save",
    "/etc/passwd",
    "C:/Users/player",
    "<unknown>/game",
    "<home>/game/./save",
    "<home>/game/stream:secret",
    "<home>/game\u0000",
  ]) {
    assert.throws(() => parseLocations([{ ...directory, path }]), path);
  }
  assert.throws(() => parseLocations([{ ...directory, platform: "Linux" }]));
  for (const exclude of [
    ["../outside"],
    ["/absolute"],
    ["a\\b"],
    ["[unclosed"],
    ["foo**bar"],
    [42],
  ])
    assert.throws(() => parseLocations([{ ...directory, exclude }]));
});

test("upload protocol requires checksums, identity, capture kind and both revisions", () => {
  const headers: Record<string, string> = {
    "x-snapshot-id": "2fb1cb34-3c6f-4abf-83be-f287a33fd4ee",
    "x-archive-sha256": "a".repeat(64),
    "x-content-sha256": "b".repeat(64),
    "x-capture-kind": "background",
    "x-expected-revision": "0",
    "x-definition-revision": "1",
  };
  assert.equal(parseUploadHeaders((key) => headers[key]).expectedRevision, 0);
  for (const [key, value] of [
    ["x-snapshot-id", "../escape"],
    ["x-archive-sha256", "md5"],
    ["x-capture-kind", "exit"],
    ["x-expected-revision", "-1"],
    ["x-definition-revision", "1.5"],
    ["x-expected-revision", "9999999999"],
  ]) {
    assert.throws(() =>
      parseUploadHeaders((name) => (name === key ? value : headers[name])),
    );
  }
  for (const namespace of ["Windows", "Linux", "macOS"])
    assert.equal(parseNamespace(namespace), namespace);
  for (const namespace of ["../Linux", "", undefined, ["Windows"]])
    assert.throws(() => parseNamespace(namespace));
});
