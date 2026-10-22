import assert from "node:assert/strict";
import test from "node:test";
import { renderGameDescription } from "../utils/game-description.ts";

const objectUrl = (id: string) => `object://localhost/${id}`;

test("description images use the desktop object loader", () => {
  const ids: string[] = [];
  const html = renderGameDescription(
    '# Ultimate Edition\n\n![Artwork](/api/v1/object/image-one "Edition")\n\n![][image]\n\n[image]: /api/v1/object/image-two',
    (id) => {
      ids.push(id);
      return objectUrl(id);
    },
  );

  assert.deepEqual(ids, ["image-one", "image-two"]);
  assert.match(html, /<h1>Ultimate Edition<\/h1>/);
  assert.ok(
    html.includes(
      '<img src="object://localhost/image-one" alt="Artwork" title="Edition" />',
    ),
  );
  assert.ok(html.includes('<img src="object://localhost/image-two" alt="" />'));
});

test("external images and ordinary links are unchanged", () => {
  const html = renderGameDescription(
    "![External](https://example.com/image.png)\n\n[Download](/api/v1/object/download)\n\n![Other](/other/image.png)",
    () => assert.fail("Only embedded Drop object images should be rewritten"),
  );

  assert.ok(html.includes('<img src="https://example.com/image.png"'));
  assert.ok(html.includes('<a href="/api/v1/object/download">Download</a>'));
  assert.ok(html.includes('<img src="/other/image.png"'));
});

test("description rendering still escapes HTML and blocks unsafe image URLs", () => {
  const html = renderGameDescription(
    '<img src="/api/v1/object/raw" onerror="alert(1)">\n\n![Unsafe](javascript:alert%281%29)',
    () => assert.fail("Raw HTML must not be treated as a Markdown image"),
  );

  assert.ok(html.includes("&lt;img"));
  assert.ok(!html.includes('<img src="/api/v1/object/raw"'));
  assert.ok(html.includes('<img src="" alt="Unsafe" />'));
});
