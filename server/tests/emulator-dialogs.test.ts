import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { parse } from "vue/compiler-sfc";

test("the emulator picker stays inside the requirements dialog context", () => {
  const source = readFileSync(
    new URL(
      "../components/GameEditor/EmulatorRequirements.vue",
      import.meta.url,
    ),
    "utf8",
  );
  const { descriptor, errors } = parse(source);
  assert.deepEqual(errors, []);
  const root = descriptor.template?.ast;
  assert.ok(root);
  const dialog = root.children.find(
    (node) => node.type === 1 && node.tag === "ModalTemplate",
  );
  assert.ok(dialog?.type === 1);
  assert.ok(
    dialog.children.some(
      (node) => node.type === 1 && node.tag === "ModalSelectLaunch",
    ),
    "A sibling picker is treated as an outside click and dismisses the editor",
  );
  assert.ok(
    !root.children.some(
      (node) => node.type === 1 && node.tag === "ModalSelectLaunch",
    ),
  );
});
