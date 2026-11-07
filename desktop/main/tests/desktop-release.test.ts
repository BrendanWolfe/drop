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
import { releaseChannel } from "../../../.github/scripts/release-channel.mjs";

const workflow = readFileSync(
  new URL("../../../.github/workflows/client-release.yml", import.meta.url),
  "utf8",
);
const script = readFileSync(
  new URL(
    "../../../.github/scripts/configure-desktop-updater.mjs",
    import.meta.url,
  ),
  "utf8",
);

function configureRelease(tag: string, branch = "develop") {
  const channel = releaseChannel({
    eventName: "workflow_dispatch",
    ref: `refs/heads/${branch}`,
    event: { inputs: { tagName: tag } },
    version: "0.4.5",
    runNumber: "42",
  });
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
        DROP_RELEASE_TAG: channel.tag,
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

test("RPM libarchive dependencies use literal names, not version expressions", () => {
  const config = JSON.parse(
    readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
  );
  // Tauri passes RPM dependencies to rpm::Dependency::any without parsing them.
  assert.deepEqual(config.bundle.linux.rpm.depends, ["libarchive"]);
  assert.deepEqual(config.bundle.linux.deb.depends, [
    "libarchive13 (>= 3.4) | libarchive13t64 (>= 3.4)",
  ]);
});

test("the embedded default tag matches the workflow build number", () => {
  assert.equal(configureRelease("").env, "DROP_RELEASE_TAG=v0.4.5-dev.42\n");
  assert.equal(
    configureRelease("", "main").env,
    "DROP_RELEASE_TAG=v0.4.5+build.42\n",
  );
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

test("both branches publish, but only main exposes an automatic-update manifest", () => {
  assert.match(workflow, /push:\s+branches: \[main, develop\]/);
  assert.ok(
    workflow.includes(
      "includeUpdaterJson: ${{ needs.prepare.outputs.prerelease == 'false' }}",
    ),
  );
  assert.ok(
    workflow.includes(
      "prerelease: ${{ needs.prepare.outputs.prerelease == 'true' }}",
    ),
  );
  assert.ok(
    workflow.includes("releaseCommitish: ${{ needs.prepare.outputs.branch }}"),
  );
});

test("automatic releases pin their tag and stay drafts until every platform succeeds", () => {
  assert.ok(
    workflow.includes(
      'git merge-base --is-ancestor HEAD "origin/$RELEASE_BRANCH"',
    ),
  );
  assert.ok(workflow.includes('"repos/$GITHUB_REPOSITORY/git/refs"'));
  assert.ok(workflow.includes('"sha=$(git rev-parse HEAD)"'));
  assert.ok(
    workflow.includes("releaseDraft: ${{ github.event_name != 'release' }}"),
  );
  assert.ok(workflow.includes("needs: [prepare, publish-tauri]"));
  assert.ok(workflow.includes('--draft=false --latest="$STABLE"'));
});

test("development server builds cannot move stable Docker tags", () => {
  const serverWorkflow = readFileSync(
    new URL("../../../.github/workflows/server-release.yml", import.meta.url),
    "utf8",
  );
  assert.match(serverWorkflow, /push:\s+branches: \[main, develop\]/);
  assert.ok(serverWorkflow.includes("flavor: latest=false"));
  assert.ok(
    serverWorkflow.includes(
      "type=raw,value=develop-${{ needs.build.outputs.version }},enable=${{ needs.build.outputs.prerelease == 'true' }}",
    ),
  );
  assert.ok(
    serverWorkflow.includes(
      "type=raw,value=${{ needs.build.outputs.version }},enable=${{ needs.build.outputs.prerelease == 'false' }}",
    ),
  );
  assert.ok(
    serverWorkflow.includes(
      "type=raw,value=latest,enable=${{ needs.build.outputs.prerelease == 'false' }}",
    ),
  );
  for (const line of serverWorkflow
    .split("\n")
    .filter((line) => line.includes("type=semver"))) {
    assert.ok(
      line.includes("enable=${{ needs.build.outputs.prerelease == 'false' }}"),
    );
  }
  assert.ok(
    serverWorkflow.includes(
      "ref: ${{ github.event_name == 'schedule' && 'develop' || github.sha }}",
    ),
  );
  assert.ok(
    serverWorkflow.includes("ref: ${{ needs.prepare.outputs.commit }}"),
  );
});
