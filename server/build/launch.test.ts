import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test, { type TestContext } from "node:test";
import { fileURLToPath } from "node:url";

const launchScript = fileURLToPath(new URL("./launch.sh", import.meta.url));
const dockerfile = fileURLToPath(new URL("../../Dockerfile", import.meta.url));

interface FixtureOptions {
  config?: boolean;
  schema?: boolean;
  migrations?: string[] | null;
  nginx?: boolean;
  pnpmExit?: number;
  nodeExit?: number;
}

interface Fixture {
  root: string;
  cwd: string;
  log: string;
  env: Record<string, string | undefined>;
}

async function writeExecutable(file: string, body: string) {
  await writeFile(file, `#!/bin/sh\n${body}\n`);
  await chmod(file, 0o755);
}

async function makeFixture(
  t: TestContext,
  options: FixtureOptions = {},
): Promise<Fixture> {
  const root = await mkdtemp(path.join(tmpdir(), "drop-launch-"));
  t.after(() => rm(root, { recursive: true, force: true }));

  const cwd = path.join(root, "app");
  const bin = path.join(root, "bin");
  const log = path.join(root, "calls.log");

  await mkdir(path.join(cwd, "prisma"), { recursive: true });
  await mkdir(bin, { recursive: true });
  await writeFile(log, "");

  if (options.config !== false) {
    await writeFile(path.join(cwd, "prisma.config.ts"), "");
  }
  if (options.schema !== false) {
    await writeFile(path.join(cwd, "prisma", "schema.prisma"), "");
  }

  if (options.migrations !== null) {
    const migrations = options.migrations ?? ["20240101000000_init"];
    await mkdir(path.join(cwd, "prisma", "migrations"), { recursive: true });

    for (const migration of migrations) {
      const directory = path.join(cwd, "prisma", "migrations", migration);
      await mkdir(directory, { recursive: true });
      await writeFile(path.join(directory, "migration.sql"), "");
    }
  }

  if (options.nginx !== false) {
    await writeFile(path.join(cwd, "nginx.conf"), "");
  }

  await writeExecutable(
    path.join(bin, "pnpm"),
    `printf 'pnpm %s\\n' "$*" >> "$LAUNCH_LOG"\nexit "\${PNPM_EXIT:-0}"`,
  );
  await writeExecutable(
    path.join(bin, "node"),
    `printf 'node %s\\n' "$*" >> "$LAUNCH_LOG"\nexit "\${NODE_EXIT:-0}"`,
  );

  const env: Record<string, string | undefined> = {
    ...process.env,
    PATH: `${bin}${path.delimiter}${process.env.PATH ?? ""}`,
    LAUNCH_LOG: log,
    PNPM_EXIT: String(options.pnpmExit ?? 0),
    NODE_EXIT: String(options.nodeExit ?? 0),
  };

  if (options.nginx !== false) {
    env.NGINX_CONFIG = path.join(cwd, "nginx.conf");
  }

  return { root, cwd, log, env };
}

function runLaunch(fixture: Fixture, overrides: Record<string, string> = {}) {
  return spawnSync("sh", [launchScript], {
    cwd: fixture.cwd,
    encoding: "utf8",
    env: { ...fixture.env, ...overrides },
  });
}

async function calls(fixture: Fixture): Promise<string[]> {
  const log = (await readFile(fixture.log, "utf8")).trim();
  return log === "" ? [] : log.split("\n");
}

test("migrates before starting the app", async (t) => {
  const fixture = await makeFixture(t);
  const result = runLaunch(fixture);

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /performing migrations/);
  assert.deepEqual(await calls(fixture), [
    "pnpm exec prisma migrate deploy --config ./prisma.config.ts",
    "node /app/app/server/index.mjs",
  ]);
});

test("a failed migration prevents the app from starting", async (t) => {
  const fixture = await makeFixture(t, { pnpmExit: 17 });
  const result = runLaunch(fixture);

  assert.equal(result.status, 17);
  assert.deepEqual(await calls(fixture), [
    "pnpm exec prisma migrate deploy --config ./prisma.config.ts",
  ]);
});

const missingPrerequisites: Array<[string, FixtureOptions]> = [
  ["prisma.config.ts", { config: false }],
  ["prisma/schema.prisma", { schema: false }],
  ["migrations", { migrations: [] }],
];

for (const [name, options] of missingPrerequisites) {
  test(`does not migrate or start the app when ${name} is missing`, async (t) => {
    const fixture = await makeFixture(t, options);
    const result = runLaunch(fixture);

    assert.notEqual(result.status, 0);
    assert.deepEqual(await calls(fixture), []);
  });
}

test("does not migrate or start the app when nginx is missing", async (t) => {
  const fixture = await makeFixture(t, { nginx: false });
  const result = runLaunch(fixture, {
    NGINX_CONFIG: path.join(fixture.root, "missing-nginx.conf"),
  });

  assert.notEqual(result.status, 0);
  assert.deepEqual(await calls(fixture), []);
});

test("honors a custom NGINX_CONFIG path", async (t) => {
  const fixture = await makeFixture(t, { nginx: false });
  const nginxConfig = path.join(fixture.root, "custom-nginx.conf");
  await writeFile(nginxConfig, "");

  const result = runLaunch(fixture, { NGINX_CONFIG: nginxConfig });

  assert.equal(result.status, 0, result.stderr);
  assert.equal((await calls(fixture)).length, 2);
});

test("runtime image carries the files and local dependencies launch needs", async () => {
  const contents = await readFile(dockerfile, "utf8");

  assert.match(contents, /RUN pnpm add prisma@\S+ dotenv@\S+/);
  assert.match(contents, /COPY --from=build-system \S*prisma\.config\.ts \.\//);
  assert.match(
    contents,
    /COPY --from=build-system \S*build\/nginx\.conf \/nginx\.conf/,
  );
  assert.match(contents, /ENV NGINX_CONFIG="\/nginx\.conf"/);
  assert.match(contents, /ENV PORT=4000/);
  assert.match(contents, /CMD \["sh", "\/app\/startup\/launch\.sh"\]/);
});
