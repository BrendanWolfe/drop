import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

test("Proton icons respect the desktop app base path", () => {
  for (const file of [
    "pages/settings.vue",
    "components/HeaderProtonSupportWidget.vue",
  ]) {
    const source = readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
    assert.match(source, /useRuntimeConfig\(\)\.app\.baseURL/);
    assert.match(source, /(?:src: protonLogoUrl|:src="protonLogoUrl")/);
    assert.doesNotMatch(source, /["']\/proton-logo\.png["']/);
  }
});

test("the bundled Proton logo is a PNG, not a missing placeholder", () => {
  const logo = readFileSync(
    new URL("../public/proton-logo.png", import.meta.url),
  );
  assert.deepEqual([...logo.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
});
