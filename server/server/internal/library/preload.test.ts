import assert from "node:assert/strict";
import test from "node:test";
import { rankPreloadCandidates } from "./preload.ts";

type Candidate = Parameters<typeof rankPreloadCandidates>[0][number];

function candidate(filename: string, match: number): Candidate {
  return {
    filename,
    match,
    platform: "Windows",
    type: "platform",
  };
}

function rank(
  entries: Array<[filename: string, match: number]>,
  setupMode = false,
) {
  return rankPreloadCandidates(
    entries.map(([filename, match]) => candidate(filename, match)),
    { setupMode },
  );
}

test("CoDWaW launch ignores the OpenAL installer", () => {
  const [selected] = rank([
    ["oalinst.exe", 1],
    ["Call of Duty World At War/CoDWaWmp.exe", 0.1],
    ["CoDWaW.exe", 0.1],
  ]);

  assert.equal(selected?.filename, "CoDWaW.exe");
});

test("SkyrimSE launch ignores _CommonRedist paths and tooling", () => {
  const [selected] = rank([
    ["_CommonRedist/vcredist/2015/vc_redist.x64.exe", 1],
    ["_CommonRedist/DirectX/Jun2010/dxsetup.exe", 1],
    ["_CommonRedist/Extras/GameTool.exe", 1],
    ["SkyrimSELauncher.exe", 0.1],
    ["SkyrimSE.exe", 0.1],
  ]);

  assert.equal(selected?.filename, "SkyrimSE.exe");
});

test("NieRAutomata launch ignores OpenAL and crash reporting tooling", () => {
  const [selected] = rank([
    ["oalinst.exe", 1],
    ["CrashReportClient.exe", 1],
    ["NieRAutomataCompat.exe", 0.1],
    ["NieRAutomata.exe", 0.1],
  ]);

  assert.equal(selected?.filename, "NieRAutomata.exe");
});

test("setup mode prefers the installer over the game executable and tooling", () => {
  const [selected] = rank(
    [
      ["Game.exe", 1],
      ["setup.exe", 0],
      ["oalinst.exe", 1],
      ["_CommonRedist/DirectX/Jun2010/dxsetup.exe", 1],
    ],
    true,
  );

  assert.equal(selected?.filename, "setup.exe");
});

test("known installer, redistributable, and crash tooling names are rejected", () => {
  const [selected] = rank([
    ["dxwebsetup.exe", 1],
    ["dxsetup.exe", 1],
    ["dotnetfx35.exe", 1],
    ["vc_redist.x64.exe", 1],
    ["unins000.exe", 1],
    ["UnityCrashHandler64.exe", 1],
    ["ExampleGame.exe", 0.1],
  ]);

  assert.equal(selected?.filename, "ExampleGame.exe");
});

test("only installer, redistributable, or crash tooling candidates are rejected", () => {
  const selected = rank([
    ["setup.exe", 1],
    ["oalinst.exe", 1],
    ["_CommonRedist/Extras/GameTool.exe", 1],
  ]);

  assert.deepEqual(selected, []);
});

test("installer matching stays anchored instead of excluding InstallationSimulator", () => {
  const [selected] = rank([
    ["InstallationSimulator.exe", 0.9],
    ["setup.exe", 1],
  ]);

  assert.equal(selected?.filename, "InstallationSimulator.exe");
});

test("setup matching does not exclude a game whose title starts with Setup", () => {
  const [selected] = rank([
    ["SetupSimulator.exe", 0.9],
    ["setup.exe", 1],
  ]);

  assert.equal(selected?.filename, "SetupSimulator.exe");
});

test("shell-escaped spaces do not add fake path depth", () => {
  const [selected] = rank([
    ["nested/Game.exe", 0.5],
    ["Very\\ Long\\ Game\\ Name.exe", 0.5],
  ]);

  assert.equal(selected?.filename, "Very\\ Long\\ Game\\ Name.exe");
});

test("redistributable paths are rejected across case and separator styles", () => {
  const [selected] = rank([
    ["_COMMONREDIST\\Tools\\Game.exe", 1],
    ["Game.exe", 0.1],
  ]);

  assert.equal(selected?.filename, "Game.exe");
});
