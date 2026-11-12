import { ArkErrors, type } from "arktype";
import { defineClientEventHandler } from "~/server/internal/clients/event-handler";
import { createDownloadManifestDetails } from "~/server/internal/library/manifest/index";

const Query = type({
  version: "string",
  previous: "string?",
  refresh: "string?",
  archive: "string?",
});

export default defineClientEventHandler(async (h3) => {
  const query = Query(getQuery(h3));
  if (query instanceof ArkErrors)
    throw createError({ statusCode: 400, message: query.summary });

  const result = await createDownloadManifestDetails(
    query.version,
    query.previous,
    query.refresh == "true",
  );
  if (
    query.archive !== "true" &&
    Object.values(result.manifests).some(
      (manifest) => manifest.version === "archive-v1",
    )
  )
    throw createError({
      statusCode: 409,
      message:
        "This version requires a desktop client with archive installation support. Update Drop Desktop and retry.",
    });
  return result;
});
