import { readFileSync, appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

/** One policy for desktop releases and server images. */
export function releaseChannel({ eventName, ref, event, version, runNumber }) {
  const branch =
    eventName === "release"
      ? event.release?.target_commitish
      : eventName === "schedule"
        ? "develop"
        : ref?.replace(/^refs\/heads\//, "");
  if (
    !["main", "develop"].includes(branch) ||
    (!["release", "schedule"].includes(eventName) &&
      ref !== `refs/heads/${branch}`)
  ) {
    throw new Error(
      "Releases must target the main or develop branch, not a tag, SHA, or feature branch.",
    );
  }
  const prerelease = branch === "develop";
  if (eventName === "release" && event.release.prerelease !== prerelease) {
    throw new Error(
      "Main releases must be stable; develop releases must be marked as prereleases.",
    );
  }
  const suppliedTag =
    eventName === "release" ? event.release.tag_name : event.inputs?.tagName;
  const tag = (
    suppliedTag ||
    (prerelease
      ? `v${version}-dev.${runNumber}`
      : `v${version}+build.${runNumber}`)
  ).replaceAll("__VERSION__", version);
  // Keep outputs single-line and prevent Docker metadata CSV/handlebars
  // injection as well as invalid/ambiguous git tag syntax.
  if (
    !tag ||
    !/^[A-Za-z0-9][A-Za-z0-9._+/-]*$/.test(tag) ||
    tag.startsWith("-") ||
    tag.startsWith("/") ||
    tag.endsWith("/") ||
    tag.endsWith(".") ||
    tag.includes("..") ||
    tag.includes("@{") ||
    tag
      .split("/")
      .some((part) => !part || part.startsWith(".") || part.endsWith(".lock"))
  ) {
    throw new Error("Release tags must be valid, single-line git tag names.");
  }
  return { branch, prerelease, tag };
}

if (
  process.argv[1] &&
  pathToFileURL(process.argv[1]).href === import.meta.url
) {
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, "utf8"));
  const { version } = JSON.parse(
    readFileSync(process.env.DROP_VERSION_FILE, "utf8"),
  );
  const result = releaseChannel({
    eventName: process.env.GITHUB_EVENT_NAME,
    ref: process.env.GITHUB_REF,
    event,
    version,
    runNumber: process.env.GITHUB_RUN_NUMBER,
  });
  appendFileSync(
    process.env.GITHUB_OUTPUT,
    `branch=${result.branch}\nprerelease=${result.prerelease}\ntag=${result.tag}\n`,
  );
}
