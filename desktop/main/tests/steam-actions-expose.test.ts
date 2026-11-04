import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const source = readFileSync(
  fileURLToPath(new URL("../components/SteamActions.vue", import.meta.url)),
  "utf8",
);

// A top-level `await` in <script setup> makes setup() async, and defineExpose
// registered after such an await is invisible to the parent's template ref.
// That made `steamActions.value.beforeUninstall` undefined and crashed the
// library page during uninstall ("Failed to load library" until restart).
// The component must therefore keep its setup synchronous: listeners are
// registered without `await`.
test("SteamActions has no top-level await before defineExpose", () => {
  const script = source.split("<script")[1] ?? "";
  assert.ok(
    script.includes("defineExpose"),
    "SteamActions must defineExpose its API",
  );
  const scriptEnd = script.indexOf("</script>");
  for (const line of script.slice(0, scriptEnd).split("\n")) {
    // Top-level statements start at column 0; `await` inside a function body
    // is always indented (the file is prettier-formatted).
    if (!/^\S/.test(line) || line.trimStart().startsWith("//")) continue;
    if (/(?<![a-zA-Z_$.])await\s/.test(line)) {
      assert.fail(
        `top-level await (${line.trim()}) makes setup() async and breaks the ` +
          "parent template ref; register listeners without await",
      );
    }
  }
});
