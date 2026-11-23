import assert from "node:assert/strict";
import {
  mkdtemp,
  mkdir,
  readFile,
  readdir,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  installerFileNames,
  listInstallerEntries,
  resolveInstallerFiles,
  snapshotInstallerFiles,
} from "./installer-files.ts";

test("snapshots only the chosen installer and arbitrary files from different library folders", async (t) => {
  const base = await mkdtemp(path.join(os.tmpdir(), "drop-installer-"));
  t.after(() => rm(base, { recursive: true, force: true }));
  const library = path.join(base, "library");
  await mkdir(path.join(library, "game", "v1"), { recursive: true });
  await mkdir(path.join(library, "shared"));
  const installer = path.join(library, "game/v1/My Setup.exe");
  await writeFile(installer, "installer", { mode: 0o755 });
  await writeFile(path.join(library, "game/v1/ignored.exe"), "not selected");
  await writeFile(path.join(library, "shared/data.bin"), "companion");
  assert.deepEqual(await listInstallerEntries(library, ""), [
    { name: "game", directory: true },
    { name: "shared", directory: true },
  ]);
  const files = await resolveInstallerFiles(library, [
    "game/v1/My Setup.exe",
    "shared/data.bin",
  ]);
  const bundle = path.join(base, "bundles", "version");
  await snapshotInstallerFiles(bundle, files);
  await writeFile(installer, "updated source");
  assert.deepEqual((await readdir(bundle)).sort(), [
    "My Setup.exe",
    "data.bin",
  ]);
  assert.equal(
    await readFile(path.join(bundle, "My Setup.exe"), "utf8"),
    "installer",
  );
});

test("rejects duplicate destination names, invalid paths and symlinks", async (t) => {
  for (const invalid of [
    "../file",
    "/etc/file",
    "a/../../b",
    "a\\b",
    "file:stream",
    ".dropdata",
    "NUL.txt",
  ])
    assert.throws(() => installerFileNames([invalid]));
  assert.throws(
    () => installerFileNames(["one/DATA.bin", "two/data.bin"]),
    /same|named/,
  );
  if (process.platform === "win32") return;
  const base = await mkdtemp(path.join(os.tmpdir(), "drop-installer-"));
  t.after(() => rm(base, { recursive: true, force: true }));
  await symlink(os.tmpdir(), path.join(base, "link"), "dir");
  await assert.rejects(
    () => resolveInstallerFiles(base, ["link/anything"]),
    /symbolic links/i,
  );
  await assert.rejects(
    () => listInstallerEntries(base, "link"),
    /symbolic links/i,
  );
});
