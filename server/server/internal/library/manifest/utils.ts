import type { JsonValue } from "@prisma/client/runtime/client";

export type DropletManifest = V2Manifest;

export type V2Manifest = {
  version: "2" | "archive-v1";
  size: number;
  key: number[];
  chunks: { [key: string]: V2ChunkData };
  archive?: { install_size: number };
};

export type V2ChunkData = {
  files: Array<V2FileEntry>;
  checksum: string;
  iv: number[];
};

export type V2FileEntry = {
  filename: string;
  start: number;
  length: number;
  permissions: number;
};

export function castManifest(manifest: JsonValue): DropletManifest {
  return (
    typeof manifest === "string" ? JSON.parse(manifest) : manifest
  ) as DropletManifest;
}
