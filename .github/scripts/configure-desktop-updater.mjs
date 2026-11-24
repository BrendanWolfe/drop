import fs from "node:fs";

const tag = process.env.DROP_RELEASE_TAG;
if (!tag || /[\r\n]/.test(tag))
  throw new Error("A single-line release tag is required");
const prerelease = process.env.DROP_DESKTOP_PRERELEASE;
if (prerelease !== "true" && prerelease !== "false")
  throw new Error("The resolved desktop release channel is required");
const development = prerelease === "true";
const pubkey = process.env.DROP_UPDATER_PUBLIC_KEY?.trim();
if (!pubkey)
  throw new Error("Set the DROP_UPDATER_PUBLIC_KEY repository secret");
fs.appendFileSync(process.env.GITHUB_ENV, `DROP_RELEASE_TAG=${tag}\n`);
fs.writeFileSync(
  "desktop/src-tauri/tauri.updater.conf.json",
  JSON.stringify({
    ...(development && {
      productName: "Drop Desktop Dev",
      identifier: "org.droposs.client.dev",
      mainBinaryName: "drop-app-dev",
      build: { features: ["development-release"] },
    }),
    bundle: { createUpdaterArtifacts: true },
    plugins: {
      updater: { pubkey },
      ...(development && {
        "deep-link": { desktop: { schemes: ["drop-dev"] } },
      }),
    },
  }),
);
