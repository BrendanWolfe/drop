import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(
  new URL("../components/GameStatusButton.vue", import.meta.url),
  "utf8",
);

test("installed games offer Change version instead of Install in the dropdown", () => {
  const template = source.split("</template>")[0]!;
  assert.match(template, /Change version…\s*<ArrowsRightLeftIcon/);
  assert.doesNotMatch(template, />\s*Install\s*</);
  assert.match(template, /@click="\(\) => emit\('install'\)"/);
});

test("uninstalled games still offer the primary Install action", () => {
  assert.match(source, /Remote: "Install"/);
  assert.match(source, /Remote: \(\) => emit\("install"\)/);
});
