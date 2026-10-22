import assert from "node:assert/strict";
import test from "node:test";
import {
  getStoreLibraryChange,
  STORE_LIBRARY_MESSAGE,
} from "../../../libraries/base/composables/store-download.ts";
import { canStartStoreDownload } from "../utils/store-download.ts";

const gameId = "c6935068-ea72-4aba-98eb-4198719eaf40";
const storeWindow = {};
const request = { type: STORE_LIBRARY_MESSAGE, gameId, download: true };

test("download requests must come from the active Store iframe", () => {
  assert.deepEqual(
    getStoreLibraryChange({ source: storeWindow, data: request }, storeWindow),
    { gameId, download: true },
  );
  assert.equal(
    getStoreLibraryChange({ source: {}, data: request }, storeWindow),
    undefined,
  );
  assert.equal(
    getStoreLibraryChange({ source: null, data: request }, undefined),
    undefined,
  );
});

test("invalid Store download requests are ignored", () => {
  for (const data of [
    null,
    undefined,
    true,
    "download",
    [],
    {},
    { type: "other", gameId },
    { type: STORE_LIBRARY_MESSAGE },
    { ...request, download: undefined },
    { ...request, download: "true" },
    { ...request, gameId: 123 },
    { ...request, gameId: "" },
    { ...request, gameId: "../../settings" },
    { ...request, gameId: `${gameId}?download=1` },
  ]) {
    assert.equal(
      getStoreLibraryChange({ source: storeWindow, data }, storeWindow),
      undefined,
    );
  }
});

test("additions without download and removals still refresh the library", () => {
  assert.deepEqual(
    getStoreLibraryChange(
      { source: storeWindow, data: { ...request, download: false } },
      storeWindow,
    ),
    { gameId, download: false },
  );
});

test("a new game downloads the latest version with one configured folder", () => {
  assert.equal(
    canStartStoreDownload({ type: "Remote" }, [{}, {}], ["/games"]),
    true,
  );
});

test("downloads wait for a folder choice or compatible version", () => {
  for (const [versions, dirs] of [
    [undefined, undefined],
    [[], ["/games"]],
    [[{}], undefined],
    [[{}], []],
    [[{}], ["/games", "/other-games"]],
  ] as const) {
    assert.equal(
      canStartStoreDownload({ type: "Remote" }, versions, dirs),
      false,
    );
  }
});

test("existing installations and active downloads are not queued again", () => {
  for (const type of [
    "Installed",
    "Queued",
    "Downloading",
    "Validating",
    "Updating",
    "Uninstalling",
    "Running",
  ]) {
    assert.equal(canStartStoreDownload({ type }, [{}], ["/games"]), false);
  }
  assert.equal(canStartStoreDownload(undefined, [{}], ["/games"]), false);
});
