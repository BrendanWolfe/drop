import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { compileScript, compileTemplate, parse } from "vue/compiler-sfc";

function readComponent(path: string) {
  return readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
}

test("console controls and both store layouts compile as Vue components", () => {
  for (const filename of [
    "components/ConsoleTypeSelector.vue",
    "components/GameEditor/Console.vue",
    "components/GameEditor/Metadata.vue",
    "components/StoreConsoleFilters.vue",
    "components/StoreView.vue",
    "pages/admin/library/import.vue",
  ]) {
    const { descriptor, errors } = parse(readComponent(filename), { filename });
    assert.deepEqual(errors, [], filename);
    const script = compileScript(descriptor, { id: filename });
    const template = compileTemplate({
      source: descriptor.template!.content,
      filename,
      id: filename,
      compilerOptions: { bindingMetadata: script.bindings },
    });
    assert.deepEqual(template.errors, [], filename);
  }
});

test("the reusable console picker offers a select, text input and non-submit create button", () => {
  const source = readComponent("components/ConsoleTypeSelector.vue");
  assert.match(source, /<select\s[^>]*v-model="selected"/);
  assert.match(source, /<input\s[^>]*v-model="newName"[^>]*type="text"/);
  assert.match(source, /<LoadingButton\s[^>]*type="button"/);
  assert.match(source, /@keydown\.enter\.prevent="createConsole"/);
});

test("mobile and desktop console subfilters share state and are nested beneath Emulated", () => {
  const source = readComponent("components/StoreView.vue");
  const filters = [...source.matchAll(/<StoreConsoleFilters\s[^>]*\/>/g)].map(
    (match) => match[0],
  );
  assert.equal(filters.length, 2);
  for (const filter of filters) {
    assert.match(filter, /section\.param === 'emulated'/);
    assert.match(filter, /option\.param === 'true'/);
    assert.match(filter, /emulatedSelected/);
    assert.match(filter, /v-model="consoleValues"/);
  }
});
