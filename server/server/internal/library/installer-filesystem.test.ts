import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { resolveFilesystemCompanion } from "./installer-filesystem.ts";
import { installerAssetDownload } from "./installer-assets.ts";

test("server-folder companions are confined, link-free and deterministic", async (t) => {
  const temp = await mkdtemp(path.join(tmpdir(), "drop-filesystem-"));
  t.after(() => rm(temp, { recursive: true, force: true }));
  const candidate = path.join(temp, "folder");
  await mkdir(candidate);
  const file = path.join(candidate, "game.bin");
  await writeFile(file, "test");
  const input = { path: candidate, kind: "folder", name: "My game" } as const;
  const noZip = async () => {
    throw new Error("Unexpected ZIP");
  };
  const first = await resolveFilesystemCompanion(input, [temp], [], noZip);
  assert.deepEqual(first.targets, ["game.bin"]);
  assert.equal(
    (
      await resolveFilesystemCompanion(
        { ...input, name: "Renamed" },
        [temp],
        [],
        noZip,
      )
    ).key,
    first.key,
  );
  await writeFile(file, "changed");
  assert.notEqual(
    (await resolveFilesystemCompanion(input, [temp], [], noZip)).key,
    first.key,
  );
  await assert.rejects(
    resolveFilesystemCompanion(input, [], [], noZip),
    /under DROP_INSTALLER/,
  );
  await assert.rejects(
    resolveFilesystemCompanion(input, [temp], [candidate], noZip),
    /inside a game library/,
  );
  await symlink(file, path.join(candidate, ".hidden"));
  await assert.rejects(
    resolveFilesystemCompanion(input, [temp], [], noZip),
    /symbolic links/,
  );
});

test("ZIP companions keep original depot names and unpack under the chosen player folder", async (t) => {
  const temp = await mkdtemp(path.join(tmpdir(), "drop-zip-"));
  t.after(() => rm(temp, { recursive: true, force: true }));
  const archive = path.join(temp, "game.zip");
  await writeFile(archive, "list via depot fixture");
  const input = { path: archive, kind: "folder", name: "Game" } as const;
  const { targets, backend } = await resolveFilesystemCompanion(
    input,
    [temp],
    [],
    async () => ["./data/eboot.bin"],
  );
  assert.equal(backend, archive);
  const entry = {
    filename: targets[0]!,
    start: 0,
    length: 4,
    permissions: 0o644,
  };
  const manifest = {
    version: "2" as const,
    size: 4,
    key: [],
    chunks: { chunk: { checksum: "hash", iv: [], files: [entry] } },
  };
  const client = installerAssetDownload("companion", manifest, {
    kind: "folder",
    target: "Game",
  });
  assert.deepEqual(client.fileList, { "Game/data/eboot.bin": "companion" });
  assert.equal(entry.filename, "./data/eboot.bin");
  await assert.rejects(
    resolveFilesystemCompanion(input, [temp], [], async () => ["../escaped"]),
    /Invalid companion path/,
  );
  await assert.rejects(
    resolveFilesystemCompanion(input, [temp], [], async () => [
      "Game.bin",
      "game.bin",
    ]),
    /duplicate/,
  );
});
