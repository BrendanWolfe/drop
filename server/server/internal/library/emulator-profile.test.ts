import assert from "node:assert/strict";
import test from "node:test";
import { ArkErrors } from "arktype";
import {
  EmulatorProfile,
  validConfigPath,
  validateEmulatorProfile,
  validateDirectOpenCommand,
} from "./emulator-profile.ts";

test("direct-open commands are optional and independent of the game launch mode", () => {
  for (const mode of ["cli", "ui"] as const) {
    for (const directOpenCommand of [
      undefined,
      "",
      "  ",
      'emulator.exe --gui --data "{emulatorDir}"',
    ]) {
      const profile = EmulatorProfile({
        mode,
        configFiles: [],
        ...(directOpenCommand === undefined ? {} : { directOpenCommand }),
      });
      assert.ok(!(profile instanceof ArkErrors));
      assert.equal(
        validateEmulatorProfile(
          profile,
          mode === "cli" ? 'emulator "{rom}"' : "emulator",
        ),
        undefined,
      );
    }
  }
});

test("direct-open commands cannot depend on a selected game or private config", () => {
  for (const placeholder of [
    "rom",
    "gameDir",
    "configDir",
    "rom:json",
    "gameDir:json",
    "configDir:json",
  ]) {
    assert.match(
      validateDirectOpenCommand(`emulator "{${placeholder}}"`)!,
      /cannot use/,
    );
    assert.match(
      validateEmulatorProfile(
        {
          mode: "cli",
          configFiles: [],
          directOpenCommand: `emulator "{${placeholder}}"`,
        },
        'emulator "{rom}"',
      )!,
      /cannot use/,
    );
  }
});

test("CLI and UI profiles have distinct launch requirements", () => {
  assert.equal(
    validateEmulatorProfile(
      { mode: "cli", configFiles: [] },
      'emulator "{rom}"',
    ),
    undefined,
  );
  assert.match(
    validateEmulatorProfile({ mode: "cli", configFiles: [] }, "emulator")!,
    /rom/,
  );
  assert.equal(
    validateEmulatorProfile({ mode: "ui", configFiles: [] }, "emulator"),
    undefined,
  );
  assert.match(
    validateEmulatorProfile(
      { mode: "ui", configFiles: [] },
      'emulator "{rom}"',
    )!,
    /must not/,
  );
});

test("configuration templates need an explicit config-directory argument", () => {
  const profile = {
    mode: "ui" as const,
    configFiles: [
      { path: "settings.json", content: '{"games":[{gameDir:json}]}' },
    ],
  };
  assert.match(validateEmulatorProfile(profile, "emulator")!, /configDir/);
  assert.equal(
    validateEmulatorProfile(
      profile,
      'emulator --config "{configDir}/settings.json"',
    ),
    undefined,
  );
});

test("configuration paths cannot escape on any supported OS", () => {
  for (const path of [
    "",
    "/tmp/config",
    "../config",
    "a/../../config",
    "C:/config",
    "C:\\config",
    "a\\..\\b",
    "a//b",
    "a/./b",
    "a\0b",
  ])
    assert.equal(validConfigPath(path), false, path);
  assert.equal(validConfigPath("nested/settings.ini"), true);
});

test("rejects duplicate paths, oversized templates and unknown modes", () => {
  assert.match(
    validateEmulatorProfile(
      {
        mode: "ui",
        configFiles: [
          { path: "config.ini", content: "" },
          { path: "CONFIG.ini", content: "" },
        ],
      },
      'emulator "{configDir}"',
    )!,
    /unique/,
  );
  assert.match(
    validateEmulatorProfile(
      {
        mode: "ui",
        configFiles: [{ path: "config.ini", content: "x".repeat(262145) }],
      },
      'emulator "{configDir}"',
    )!,
    /256 KiB/,
  );
  assert.ok(
    EmulatorProfile({ mode: "shell", configFiles: [] }) instanceof ArkErrors,
  );
  assert.match(
    validateEmulatorProfile(
      {
        mode: "ui",
        configFiles: Array.from({ length: 33 }, (_, index) => ({
          path: `${index}.ini`,
          content: "",
        })),
      },
      'emulator "{configDir}"',
    )!,
    /32/,
  );
  assert.match(
    validateEmulatorProfile(
      {
        mode: "ui",
        configFiles: [{ path: "config.ini", content: "€".repeat(90000) }],
      },
      'emulator "{configDir}"',
    )!,
    /256 KiB/,
  );
});
