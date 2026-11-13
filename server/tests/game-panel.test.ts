import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { toPng } from "jdenticon";
import { compileScript, compileTemplate, parse } from "vue/compiler-sfc";
import {
  isGeneratedArtworkHeader,
  isGeneratedCover,
} from "../utils/game-panel-artwork.ts";

test("generated no-cover artwork is recognized from its PNG metadata", () => {
  for (const seed of ["Manual Game", "12345", ""])
    assert.equal(
      isGeneratedArtworkHeader(toPng(seed, 512).subarray(0, 256)),
      true,
    );
});

test("ordinary, incomplete and malformed image headers are not mistaken for placeholders", () => {
  const png = toPng("Real artwork", 512);
  const ordinary = Buffer.from(png);
  ordinary.write("PhotoTool", ordinary.indexOf("Jdenticon"));
  assert.equal(isGeneratedArtworkHeader(ordinary.subarray(0, 256)), false);
  assert.equal(
    isGeneratedArtworkHeader(new TextEncoder().encode("Software\0Jdenticon")),
    false,
  );
  for (const length of [0, 7, 8, 12, 40, 60])
    assert.equal(isGeneratedArtworkHeader(png.subarray(0, length)), false);
  const malformed = Buffer.from(png.subarray(0, 256));
  malformed.writeUInt32BE(0xffffffff, 8);
  assert.equal(isGeneratedArtworkHeader(malformed), false);
});

test("placeholder checks reuse the browser cache, stop after the header and deduplicate tile requests", async (t) => {
  let canceled = false;
  const fetcher = t.mock.method(
    globalThis,
    "fetch",
    async () =>
      new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(toPng("Placeholder", 512));
          },
          cancel() {
            canceled = true;
          },
        }),
      ),
  );
  const src = "/api/v1/object/placeholder-test";
  assert.deepEqual(
    await Promise.all([isGeneratedCover(src), isGeneratedCover(src)]),
    [true, true],
  );
  assert.equal(await isGeneratedCover(src), true);
  assert.equal(fetcher.mock.callCount(), 1);
  assert.deepEqual(fetcher.mock.calls[0]?.arguments, [
    src,
    { cache: "force-cache" },
  ]);
  assert.equal(canceled, true);
});

test("failed artwork checks do not break a tile and can be retried", async (t) => {
  let attempts = 0;
  t.mock.method(globalThis, "fetch", async () => {
    attempts++;
    if (attempts === 1) return new Response(null, { status: 404 });
    return new Response(new Uint8Array(toPng("Retry", 512)));
  });
  assert.equal(await isGeneratedCover("/api/v1/object/retry-test"), false);
  assert.equal(await isGeneratedCover("/api/v1/object/retry-test"), true);
  assert.equal(attempts, 2);
});

test("tile titles and their gradient reveal together on hover or focus, with fallback and touch exceptions", () => {
  const filename = "GamePanel.vue";
  const source = readFileSync(
    new URL("../components/GamePanel.vue", import.meta.url),
    "utf8",
  );
  const { descriptor, errors } = parse(source, { filename });
  assert.deepEqual(errors, []);
  const script = compileScript(descriptor, { id: filename });
  const template = compileTemplate({
    source: descriptor.template!.content,
    filename,
    id: filename,
    compilerOptions: { bindingMetadata: script.bindings },
  });
  assert.deepEqual(template.errors, []);
  assert.match(source, /'title-on-hover': !fallbackArtwork/);
  assert.match(source, /@media \(hover: hover\)/);
  assert.match(source, /\.title-on-hover:hover \.tile-decoration/);
  assert.match(source, /\.title-on-hover:focus-within \.tile-decoration/);
  assert.equal((source.match(/class="tile-decoration /g) ?? []).length, 2);
  assert.match(source, /@error="coverFailed = true"/);
  assert.doesNotMatch(
    descriptor.template!.content,
    /mShortDescription|<p[\s>]/,
  );
});
