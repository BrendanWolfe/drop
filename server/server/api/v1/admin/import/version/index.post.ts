import { type } from "arktype";
import { Platform } from "~/prisma/client/enums";
import { readDropValidatedBody, throwingArktype } from "~/server/arktype";
import aclManager from "~/server/internal/acls";
import libraryManager from "~/server/internal/library";
import { EmulatorProfile } from "~/server/internal/library/emulator-profile";

export const ImportVersion = type({
  id: "string",
  version: type({
    type: "'depot' | 'local'",
    identifier: "string",
    name: "string",
  }),
  displayName: "string?",

  launches: type({
    platform: type.valueOf(Platform),
    protonCompatible: "boolean?",
    name: "string",
    launch: "string",
    umuId: "string?",
    emulatorId: "string?",
    suggestions: "string[]?",
    "emulatorProfile?": EmulatorProfile,
  }).array(),

  setups: type({
    platform: type.valueOf(Platform),
    protonCompatible: "boolean?",
    launch: "string",
    emulatorId: "string.uuid?",
  }).array(),

  onlySetup: "boolean = false",
  delta: "boolean = false",
  archiveDownload: "boolean = false",
  "installerFlow?": type({
    "launchPath?": "string <= 1024",
    libraryInputs: type({
      gameId: "string.uuid",
      versionId: "string.uuid",
      kind: "'file' | 'folder'",
      path: "string <= 1024",
      name: "string <= 240",
    })
      .array()
      .atMostLength(24),
    filesystemInputs: type({
      path: "string <= 4096",
      kind: "'file' | 'folder'",
      name: "string <= 240",
      "archiveEntry?": "string <= 1024",
    })
      .array()
      .atMostLength(24),
  }),
  "installerPackage?": type({
    executable: "string",
    "launchPath?": "string <= 1024",
    files: "string[] <= 1000",
    platform: type.valueOf(Platform),
    protonCompatible: "boolean?",
    emulatorId: "string.uuid?",
    "libraryInputs?": type({
      gameId: "string.uuid",
      versionId: "string.uuid",
      kind: "'file' | 'folder'",
      path: "string <= 1024",
    })
      .array()
      .atMostLength(24),
  }),

  requiredContent: type("string")
    .array()
    .default(() => []),
}).configure(throwingArktype);

export default defineEventHandler(async (h3) => {
  const allowed = await aclManager.allowSystemACL(h3, ["import:version:new"]);
  if (!allowed) throw createError({ statusCode: 403 });

  const body = await readDropValidatedBody(h3, ImportVersion);

  const taskId = await libraryManager.importVersion(
    body.id,
    body.version,
    body,
  );
  if (!taskId)
    throw createError({
      statusCode: 400,
      statusMessage: "Invalid options for import",
    });

  return { taskId: taskId };
});
