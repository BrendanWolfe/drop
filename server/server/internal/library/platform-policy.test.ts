import assert from "node:assert/strict";
import test from "node:test";
import { allowsProton, downloadLaunches } from "./platform-policy.ts";

test("Proton eligibility is explicit and never inferred for native Linux builds", () => {
  assert.equal(allowsProton({ platform: "Windows" }), false);
  assert.equal(
    allowsProton({ platform: "Windows", protonCompatible: false }),
    false,
  );
  assert.equal(
    allowsProton({ platform: "Windows", protonCompatible: true }),
    true,
  );
  assert.equal(
    allowsProton({ platform: "Linux", protonCompatible: true }),
    false,
  );
  assert.equal(
    allowsProton({ platform: "macOS", protonCompatible: true }),
    false,
  );
});

test("Windows emulator dependencies also need Proton eligibility", () => {
  const launch = { platform: "Windows", protonCompatible: true };
  assert.equal(
    allowsProton({
      ...launch,
      emulator: { platform: "Windows", protonCompatible: true },
    }),
    true,
  );
  assert.equal(
    allowsProton({
      ...launch,
      emulator: { platform: "Windows", protonCompatible: false },
    }),
    false,
  );
  assert.equal(
    allowsProton({
      ...launch,
      emulator: { platform: "Linux", protonCompatible: true },
    }),
    false,
  );
});

test("installer platforms do not advertise nonexistent native game builds", () => {
  const launches = [{ platform: "Windows", protonCompatible: true }];
  const setups = [{ platform: "Linux" }];
  assert.deepEqual(
    downloadLaunches({ onlySetup: false, launches, setups }),
    launches,
  );
  assert.deepEqual(
    downloadLaunches({ onlySetup: true, launches, setups }),
    setups,
  );
});
