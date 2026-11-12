import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mock, test } from "node:test";

const moduleMock = (
  mock as unknown as {
    module: (
      specifier: string,
      options: {
        defaultExport?: unknown;
        namedExports?: Record<string, unknown>;
      },
    ) => void;
  }
).module.bind(mock);

const cache = new Map<string, unknown>();
moduleMock(new URL("../cache/index.ts", import.meta.url).href, {
  defaultExport: {
    createCache: () => ({
      has: async (key: string) => cache.has(key),
      get: async (key: string) => cache.get(key),
      set: async (key: string, value: unknown) => cache.set(key, value),
    }),
  },
});

type Version = { delta: boolean; dropletManifest: string };
let version: Version | null;
const findUnique = mock.fn(async (_query: unknown) => version);
moduleMock(new URL("../db/database.ts", import.meta.url).href, {
  defaultExport: { gameVersion: { findUnique } },
});

let manifestDetails = { installSize: 150, downloadSize: 200 };
const createDownloadManifestDetails = mock.fn(
  async (_versionId: string, _previousId?: string) => manifestDetails,
);
moduleMock(new URL("../library/manifest/index.ts", import.meta.url).href, {
  namedExports: { createDownloadManifestDetails },
});

const { default: gameSizeManager } = await import("./index.ts");

function reset(size = 123) {
  cache.clear();
  findUnique.mock.resetCalls();
  createDownloadManifestDetails.mock.resetCalls();
  // No chunks are needed: size reads must not enumerate the manifest's files.
  version = { delta: false, dropletManifest: JSON.stringify({ size }) };
  manifestDetails = { installSize: 150, downloadSize: 200 };
}

test("full version sizes use the stored total without constructing a download manifest", async () => {
  reset(5_000_000_000);
  assert.deepEqual(await gameSizeManager.getVersionSize("full"), {
    versionId: "full",
    installSize: 5_000_000_000,
    downloadSize: 5_000_000_000,
  });
  assert.deepEqual(findUnique.mock.calls[0]?.arguments[0], {
    where: { versionId: "full" },
    select: { delta: true, dropletManifest: true },
  });
  assert.equal(createDownloadManifestDetails.mock.callCount(), 0);

  await gameSizeManager.getVersionSize("full");
  assert.equal(findUnique.mock.callCount(), 1, "the size result is cached");
});

test("empty full versions retain zero sizes", async () => {
  reset(0);
  assert.deepEqual(await gameSizeManager.getVersionSize("empty"), {
    versionId: "empty",
    installSize: 0,
    downloadSize: 0,
  });
  assert.equal(createDownloadManifestDetails.mock.callCount(), 0);
});

test("archive version sizes distinguish compressed download bytes from extracted files", async () => {
  reset();
  version!.dropletManifest = JSON.stringify({
    version: "archive-v1",
    size: 100,
    archive: { install_size: 500 },
  });
  assert.deepEqual(await gameSizeManager.getVersionSize("archive"), {
    versionId: "archive",
    installSize: 500,
    downloadSize: 100,
  });
  assert.equal(createDownloadManifestDetails.mock.callCount(), 0);
  assert.equal(await gameSizeManager.getVersionDiskSize("archive"), 100);
});

test("delta versions still calculate sizes across their base versions", async () => {
  reset();
  version!.delta = true;
  assert.deepEqual(await gameSizeManager.getVersionSize("delta"), {
    versionId: "delta",
    ...manifestDetails,
  });
  assert.deepEqual(createDownloadManifestDetails.mock.calls[0]?.arguments, [
    "delta",
    undefined,
  ]);
});

test("update downloads retain their separate calculations and cache keys", async () => {
  reset();
  await gameSizeManager.getVersionSize("full");
  assert.deepEqual(await gameSizeManager.getVersionSize("full", "previous"), {
    versionId: "full",
    ...manifestDetails,
  });
  assert.deepEqual(createDownloadManifestDetails.mock.calls[0]?.arguments, [
    "full",
    "previous",
  ]);
  assert.equal(findUnique.mock.callCount(), 1);

  await gameSizeManager.getVersionSize("full", "previous");
  assert.equal(createDownloadManifestDetails.mock.callCount(), 1);
});

test("missing versions return null without constructing a manifest", async () => {
  reset();
  version = null;
  assert.equal(await gameSizeManager.getVersionSize("missing"), null);
  assert.equal(createDownloadManifestDetails.mock.callCount(), 0);
});

test("invalid manifests retain the null-on-failure behavior", async () => {
  reset();
  version!.dropletManifest = "invalid json";
  assert.equal(await gameSizeManager.getVersionSize("invalid"), null);
});

test("game detail queries omit the unused per-file data", () => {
  const source = readFileSync(
    new URL("../../api/v1/games/[id]/index.get.ts", import.meta.url),
    "utf8",
  );
  assert.match(
    source,
    /omit:\s*\{\s*dropletManifest: true,\s*fileList: true,\s*negativeFileList: true,\s*\}/,
  );
});
