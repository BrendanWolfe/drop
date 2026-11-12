import assert from "node:assert/strict";
import { mock, test } from "node:test";
import { castManifest } from "./utils.ts";

const moduleMock = (
  mock as unknown as {
    module: (specifier: string, options: { defaultExport: unknown }) => void;
  }
).module.bind(mock);
const cache = new Map<string, unknown>();
moduleMock(new URL("../../cache/index.ts", import.meta.url).href, {
  defaultExport: {
    createCache: () => ({
      has: async (key: string) => cache.has(key),
      get: async (key: string) => cache.get(key),
      set: async (key: string, value: unknown) => cache.set(key, value),
    }),
  },
});
const archive = {
  version: "archive-v1",
  size: 100,
  archive: { install_size: 500 },
  key: [],
  chunks: {
    chunk: {
      checksum: "hash",
      iv: [],
      files: [
        { filename: ".drop-archive", start: 0, length: 100, permissions: 384 },
      ],
    },
  },
};
let version = {
  versionId: "version",
  delta: false,
  versionIndex: 1,
  fileList: [".drop-archive"],
  negativeFileList: [],
  gameId: "game",
  dropletManifest: JSON.stringify(archive),
};
let base: typeof version | null = null;
const findUnique = mock.fn(async () => version);
const findFirst = mock.fn(async () => base);
moduleMock(new URL("../../db/database.ts", import.meta.url).href, {
  defaultExport: { gameVersion: { findUnique, findFirst } },
});
Object.assign(globalThis, {
  createError: (options: { message: string }) => new Error(options.message),
});
const { createDownloadManifestDetails } = await import("./index.ts");

test("archive downloads retain every compressed byte range and use extracted install size", async () => {
  cache.clear();
  const result = await createDownloadManifestDetails("version", "old-version");
  assert.deepEqual(result, {
    manifests: { version: archive },
    fileList: { ".drop-archive": "version" },
    installSize: 500,
    downloadSize: 100,
  });
  assert.equal(
    findUnique.mock.callCount(),
    1,
    "previous versions must not filter an archive",
  );
  assert.equal(findFirst.mock.callCount(), 0);
});

test("archive manifests remain compatible with JSON strings and JSON objects", () => {
  assert.deepEqual(castManifest(JSON.stringify(archive)), archive);
  assert.deepEqual(castManifest(archive), archive);
});

test("archive extraction size is required and must be a nonnegative safe integer", async () => {
  for (const metadata of [
    undefined,
    { install_size: -1 },
    { install_size: 0.5 },
  ]) {
    cache.clear();
    version = {
      ...version,
      delta: false,
      dropletManifest: JSON.stringify({ ...archive, archive: metadata }),
    };
    await assert.rejects(
      createDownloadManifestDetails("version"),
      /Invalid archive manifest/,
    );
  }
});

test("archive manifests cannot be delta versions", async () => {
  cache.clear();
  version = {
    ...version,
    delta: true,
    dropletManifest: JSON.stringify(archive),
  };
  await assert.rejects(
    createDownloadManifestDetails("version"),
    /Invalid archive manifest/,
  );
});

test("normal deltas cannot use archive packages as their base", async () => {
  cache.clear();
  base = { ...version, delta: false, dropletManifest: JSON.stringify(archive) };
  version = {
    ...version,
    delta: true,
    dropletManifest: JSON.stringify({
      ...archive,
      version: "2",
      archive: undefined,
    }),
  };
  await assert.rejects(
    createDownloadManifestDetails("version"),
    /Delta updates cannot be applied to archive/,
  );
});
