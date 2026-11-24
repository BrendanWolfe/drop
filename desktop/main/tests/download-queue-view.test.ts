import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(
  new URL("../pages/queue.vue", import.meta.url),
  "utf8",
);

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

test("speed bars are square-cornered and keep a fixed width and gap", () => {
  const bar = source.match(
    /<div\b[^>]*v-for="\(bar, index\) in visibleSpeedHistory"[^>]*>/,
  )?.[0];
  assert.ok(bar);
  assert.match(bar, /\bshrink-0\b/);
  assert.doesNotMatch(bar, /\bflex-1\b|\brounded/);
  assert.match(bar, /width: `\$\{speedBarWidth\}px`/);
  assert.match(source, /gap: `\$\{speedBarGap\}px`/);
});
