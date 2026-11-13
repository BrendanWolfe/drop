import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("../pages/queue.vue", import.meta.url), "utf8");

test("the active drop target has no highlighted wrapper", () => {
  const target = source.match(/<draggable\b[^>]*data-slot="active"[^>]*>/)?.[0];
  assert.ok(target);
  assert.doesNotMatch(target, /bg-blue|ring-|\bp-2\b/);
});

test("pause and resume use an accessible icon button with a tooltip", () => {
  const button = source.match(
    /<button\b[^>]*@click="toggleDownloads"[^>]*>[\s\S]*?<\/button>/,
  )?.[0];
  assert.ok(button);
  assert.match(button, /:aria-label="downloadControlLabel"/);
  assert.match(button, /:title="downloadControlLabel"/);
  assert.match(button, /\bsize-10\b/);
  assert.match(button, /PauseIcon/);
  assert.match(button, /PlayIcon/);
  assert.doesNotMatch(button, /\{\{\s*downloadControlLabel\s*\}\}/);
});

test("speed bars and their gaps scale to the full graph width", () => {
  const bar = source.match(
    /<div\b[^>]*v-for="\(bar, index\) in speedHistory"[^>]*>/,
  )?.[0];
  assert.ok(bar);
  assert.match(bar, /min-w-0 flex-1/);
  assert.doesNotMatch(bar, /w-\[3px\]|shrink-0/);
  assert.match(source, /absolute inset-0 flex items-end gap-\[0\.1%\] pt-2/);
});
