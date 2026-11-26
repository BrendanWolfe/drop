import {
  VersionQuerySchema,
  VersionResponse_LibrarySource_LibraryBackend,
  VersionResponse_LibrarySourceSchema,
  VersionResponse_Manifest_ChunkData_FileEntrySchema,
  VersionResponse_Manifest_ChunkDataSchema,
  VersionResponse_ManifestSchema,
  VersionResponseSchema,
} from "../../proto/torrential/proto/version_pb";
import { castManifest } from "../../library/manifest/utils";
import { companionArchiveSuffix } from "../../library/manifest/companion-archive";
import { LibraryBackend } from "~/prisma/client/client";
import { create, fromBinary } from "@bufbuild/protobuf";
import prisma from "../../db/database";
import { installerBundleRoot } from "../../library/installer-bundles";
import { installerAssetPrefix } from "../../library/installer-assets";
import { libraryEntry } from "../../library/installer-files";
import {
  configuredCompanionRoots,
  filesystemCompanionSource,
} from "../../library/installer-filesystem";
import { defineQueryProcessor } from "./utils";
import {
  DropBoundType,
  TorrentialBoundType,
} from "../../proto/torrential/proto/core_pb";

export default defineQueryProcessor({
  queryType: DropBoundType.VERSION_QUERY,
  run: async (query) => {
    const queryData = fromBinary(VersionQuerySchema, query.data);
    if (queryData.versionId.startsWith(installerAssetPrefix)) {
      const asset = await prisma.installerAsset.findUnique({
        where: { id: queryData.versionId },
        select: {
          manifest: true,
          sourcePath: true,
          filesystemKind: true,
          filesystemEntry: true,
          version: { select: { game: { select: { library: true } } } },
        },
      });
      if (!asset)
        throw createError({
          statusCode: 404,
          message: "Installer companion not found",
        });
      const library = asset.version.game.library;
      if (asset.filesystemKind) {
        const { default: manager } = await import("../../library");
        const { backend } = await filesystemCompanionSource(
          asset.sourcePath,
          asset.filesystemKind,
          configuredCompanionRoots(),
          await manager.companionLibraryRoots(),
          asset.filesystemEntry,
        );
        return versionResponse(
          castManifest(asset.manifest),
          {
            options: JSON.stringify({ baseDir: backend, installerAsset: true }),
            id: library.id,
            backend:
              VersionResponse_LibrarySource_LibraryBackend.FLAT_FILESYSTEM,
          },
          "",
          "",
        );
      }
      const options = library.options as { baseDir: string };
      const source = await libraryEntry(options.baseDir, asset.sourcePath);
      if (!source.stat.isDirectory() && !source.stat.isFile())
        throw createError({ statusCode: 404 });
      return versionResponse(
        castManifest(asset.manifest),
        {
          options: JSON.stringify({
            baseDir: options.baseDir,
            installerAsset: true,
          }),
          id: library.id,
          backend: VersionResponse_LibrarySource_LibraryBackend.FLAT_FILESYSTEM,
        },
        asset.sourcePath,
        "",
      );
    }
    const companion = queryData.versionId.endsWith(companionArchiveSuffix);

    const version = await prisma.gameVersion.findUnique({
      where: {
        versionId: companion
          ? queryData.versionId.slice(0, -companionArchiveSuffix.length)
          : queryData.versionId,
      },
      select: {
        dropletManifest: true,
        companionArchiveManifest: true,
        versionPath: true,
        installerBundleId: true,
        game: {
          select: {
            library: true,
            libraryPath: true,
          },
        },
      },
    });
    if (!version || (companion && !version.companionArchiveManifest))
      throw createError({
        statusCode: 404,
        message: "Game version not found",
      });

    const manifest = castManifest(
      companion ? version.companionArchiveManifest : version.dropletManifest,
    );

    const mapEnum = (v: LibraryBackend) => {
      switch (v) {
        case LibraryBackend.Filesystem:
          return VersionResponse_LibrarySource_LibraryBackend.FILESYSTEM;
        case LibraryBackend.FlatFilesystem:
          return VersionResponse_LibrarySource_LibraryBackend.FLAT_FILESYSTEM;
      }
    };

    const source = version.installerBundleId
      ? {
          options: JSON.stringify({ baseDir: installerBundleRoot() }),
          id: version.game.library.id,
          backend: VersionResponse_LibrarySource_LibraryBackend.FLAT_FILESYSTEM,
        }
      : {
          options: JSON.stringify(version.game.library.options),
          id: version.game.library.id,
          backend: mapEnum(version.game.library.backend),
        };
    return versionResponse(
      manifest,
      source,
      version.installerBundleId ?? version.game.libraryPath,
      version.versionPath!,
    );
  },
});

function versionResponse(
  manifest: ReturnType<typeof castManifest>,
  source: {
    options: string;
    id: string;
    backend: VersionResponse_LibrarySource_LibraryBackend;
  },
  libraryPath: string,
  versionPath: string,
) {
  return {
    type: TorrentialBoundType.VERSION_RESPONSE,
    schema: VersionResponseSchema,
    data: create(VersionResponseSchema, {
      manifest: create(VersionResponse_ManifestSchema, {
        version: manifest.version,
        size: BigInt(manifest.size),
        key: Buffer.from(manifest.key),
        chunks: Object.fromEntries(
          Object.entries(manifest.chunks).map(([id, chunk]) => [
            id,
            create(VersionResponse_Manifest_ChunkDataSchema, {
              checksum: chunk.checksum,
              iv: Buffer.from(chunk.iv),
              files: chunk.files.map((file) =>
                create(VersionResponse_Manifest_ChunkData_FileEntrySchema, {
                  filename: file.filename,
                  start: BigInt(file.start),
                  length: BigInt(file.length),
                  permissions: file.permissions,
                }),
              ),
            }),
          ]),
        ),
      }),
      source: create(VersionResponse_LibrarySourceSchema, source),
      libraryPath,
      versionPath,
    }),
  };
}
