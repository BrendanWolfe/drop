import assert from "node:assert/strict";
import {
  mkdtemp,
  mkdir,
  readdir,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  browseInstallerZip,
  installerAssetDownload,
  resolveInstallerAsset,
  validateAssetManifest,
} from "./installer-assets.ts";

test("loose files and folders are indexed in place and never copied into an installer bundle", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "drop-asset-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(path.join(root, "data/maps"), { recursive: true });
  await writeFile(path.join(root, "giant.nup"), "fixture");
  await writeFile(path.join(root, "data/maps/level.bin"), "fixture");
  const noArchive = async () => {
    throw new Error("Not a ZIP");
  };
  const file = await resolveInstallerAsset(
    root,
    { source: "giant.nup", kind: "file" },
    noArchive,
  );
  assert.equal(file.source, root);
  assert.equal(file.sourcePath, "");
  assert.deepEqual(file.targets, ["giant.nup"]);
  const folder = await resolveInstallerAsset(
    root,
    { source: "data", kind: "folder" },
    noArchive,
  );
  assert.deepEqual(folder.targets, ["data/maps/level.bin"]);
  assert.deepEqual((await readdir(root)).sort(), ["data", "giant.nup"]);
  if (process.platform !== "win32") {
    await symlink(
      path.join(root, "giant.nup"),
      path.join(root, "data/link.nup"),
    );
    await assert.rejects(
      resolveInstallerAsset(
        root,
        { source: "data", kind: "folder" },
        noArchive,
      ),
      /symbolic links/,
    );
  }
});

test("ZIP browsing and download manifests include only the selected entry or subtree", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "drop-zip-asset-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(path.join(root, "content.zip"), "list supplied by depot");
  const list = async () => [
    "./maps/level.bin",
    "maps-extra/bogus.bin",
    "ignore.txt",
    "../escape",
  ];
  const asset = await resolveInstallerAsset(
    root,
    { source: "content.zip", kind: "folder", archiveEntry: "maps" },
    list,
  );
  assert.deepEqual(asset.targets, ["./maps/level.bin"]);
  assert.equal(asset.targetPath, "maps");
  assert.deepEqual(
    (await browseInstallerZip(root, "content.zip", "", list)).map(
      (entry) => entry.name,
    ),
    ["maps", "maps-extra", "ignore.txt"],
  );
  const raw = JSON.stringify({
    version: "2",
    size: 3,
    chunks: {
      chunk: { files: [{ filename: "./maps/level.bin", start: 0, length: 3 }] },
    },
  });
  assert.throws(() => validateAssetManifest(raw, ["ignore.txt"]), /exactly/);
  const manifest = validateAssetManifest(raw, asset.targets);
  const download = installerAssetDownload("asset", manifest);
  assert.deepEqual(download.fileList, { "maps/level.bin": "asset" });
  assert.equal(manifest.chunks.chunk!.files[0]!.filename, "./maps/level.bin");
});
