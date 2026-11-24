import assert from "node:assert/strict";
import test from "node:test";
import {
  hasLibraryInput,
  installedFilePaths,
  listInputDirectory,
  normalizeInstallerLaunchPath,
} from "./installer-input-paths.ts";

test("installer launch paths accept Windows separators but cannot escape the game folder", () => {
  assert.equal(
    normalizeInstallerLaunchPath(" .\\bin\\My Game.exe "),
    "bin/My Game.exe",
  );
  assert.equal(normalizeInstallerLaunchPath(""), undefined);
  for (const path of [
    "C:\\game.exe",
    "/game.exe",
    "\\\\share\\game.exe",
    "bin/../game.exe",
    "bin/",
  ])
    assert.throws(() => normalizeInstallerLaunchPath(path), /relative/);
});

test("can select a ROM, its parent folder, or the entire installed game", () => {
  const files = installedFilePaths([
    "./roms/Game Name.z64",
    "extras/guide.pdf",
    "README.txt",
  ]);
  assert.equal(
    hasLibraryInput(files, { kind: "file", path: "roms/Game Name.z64" }),
    true,
  );
  assert.equal(hasLibraryInput(files, { kind: "folder", path: "roms" }), true);
  assert.equal(hasLibraryInput(files, { kind: "folder", path: "" }), true);
  assert.equal(hasLibraryInput(files, { kind: "file", path: "" }), false);
  assert.equal(hasLibraryInput(files, { kind: "folder", path: "rom" }), false);
  assert.equal(hasLibraryInput(files, { kind: "file", path: "roms" }), false);
  assert.deepEqual(listInputDirectory(files, ""), [
    { kind: "folder", name: "extras" },
    { kind: "folder", name: "roms" },
    { kind: "file", name: "README.txt" },
  ]);
  assert.deepEqual(listInputDirectory(files, "roms", "GAME"), [
    { kind: "file", name: "Game Name.z64" },
  ]);
});

test("does not offer unsafe or Drop-owned paths", () => {
  const invalid = [
    "../rom.z64",
    "/elsewhere.rom",
    "C:/rom",
    ".dropdata",
    "data/.private/main.rom",
    "./../escape",
    "roms\\game.rom",
  ];
  assert.deepEqual([...installedFilePaths(invalid)], []);
  for (const path of invalid)
    assert.equal(
      hasLibraryInput(new Set(invalid), { kind: "file", path }),
      false,
    );
});
