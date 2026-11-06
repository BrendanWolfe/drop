import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const workflow = readFileSync(
  new URL("../../../.github/workflows/client-release.yml", import.meta.url),
  "utf8",
);
const script = workflow
  .match(/node --input-type=module <<'NODE'\r?\n([\s\S]*?)\r?\n\s*NODE/)?.[1]
  ?.replace(/^ {10}/gm, "");
assert.ok(script, "The desktop release configuration script must exist");

function configureRelease(tag: string) {
  const directory = mkdtempSync(join(tmpdir(), "drop-release-test-"));
  try {
    mkdirSync(join(directory, "desktop/src-tauri"), { recursive: true });
    writeFileSync(
      join(directory, "desktop/src-tauri/tauri.conf.json"),
      JSON.stringify({ version: "0.4.5" }),
    );
    const envFile = join(directory, "github-env");
    execFileSync(process.execPath, ["--input-type=module"], {
      input: script,
      cwd: directory,
      env: {
        ...process.env,
        DROP_RELEASE_TAG: tag,
        DROP_UPDATER_PUBLIC_KEY: "  fixture-public-key\n",
        GITHUB_ENV: envFile,
        GITHUB_RUN_NUMBER: "42",
      },
      stdio: ["pipe", "pipe", "pipe"],
    });
    return {
      config: JSON.parse(
        readFileSync(
          join(directory, "desktop/src-tauri/tauri.updater.conf.json"),
          "utf8",
        ),
      ),
      env: readFileSync(envFile, "utf8"),
    };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test("release bundling receives the verification key as well as signed-artifact configuration", () => {
  const { config } = configureRelease("v0.4.5-build.42");
  assert.equal(config.bundle.createUpdaterArtifacts, true);
  assert.equal(config.plugins.updater.pubkey, "fixture-public-key");
});

test("the embedded default tag matches the workflow build number", () => {
  assert.equal(configureRelease("").env, "DROP_RELEASE_TAG=v0.4.5-build.42\n");
});

test("explicit release tags resolve the version placeholder", () => {
  assert.equal(
    configureRelease("v__VERSION__-test").env,
    "DROP_RELEASE_TAG=v0.4.5-test\n",
  );
});

test("release tags cannot inject extra GitHub environment variables", () => {
  assert.throws(() => configureRelease("v0.4.5\nOTHER=value"));
});

test("the updater bridge remains a draft until all platforms finish, then targets stable main", () => {
  assert.ok(
    workflow.includes("releaseDraft: ${{ github.event_name != 'release' }}"),
  );
  assert.ok(workflow.includes("includeUpdaterJson:"));
  assert.ok(workflow.includes("needs: publish-tauri"));
  assert.ok(
    workflow.includes(
      "if: github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main'",
    ),
  );
  assert.ok(
    workflow.includes(
      "--target main --draft=false --prerelease=false --latest",
    ),
  );
  assert.ok(workflow.includes("GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}"));
});
