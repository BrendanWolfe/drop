import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

test("installed games expose an explicit integrity action", () => {
  const button = read("../components/GameStatusButton.vue");
  assert.match(button, /Verify file integrity…/);
  assert.match(button, /emit\('verify'\)/);
  const page = read("../pages/library/[id]/index.vue");
  assert.match(page, /@verify="integrityOpen = true"/);
  assert.match(page, /<FileIntegrityModal/);
});

test("verification confirms repair scope and uses the existing queue", () => {
  const modal = read("../components/FileIntegrityModal.vue");
  assert.match(modal, /DialogTitle/);
  assert.match(modal, /invoke\("verify_file_integrity"/);
  assert.match(modal, /versionId: props.versionId/);
  assert.match(modal, /targetPlatform: props.targetPlatform/);
  assert.match(modal, /router.push\("\/queue"\)/);
  assert.match(modal, /back up your mods first/);
  assert.match(modal, /role="alert"/);
});

test("emulator verification targets an exact installed build", () => {
  const page = read("../pages/emulators.vue");
  assert.match(page, /:version-id="integrityTarget.versionId"/);
  assert.match(page, /:target-platform="integrityTarget.platform"/);
  assert.match(page, /:disabled="version.busy"/);
});
