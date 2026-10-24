import assert from "node:assert/strict";
import test from "node:test";
import { importedImageFields, removeImageFromCarousel } from "./images.ts";

test("imported games seed the carousel from the image library", () => {
  const images = ["screenshot-1", "screenshot-2", "screenshot-3"];

  assert.deepEqual(importedImageFields(images), {
    mImageLibraryObjectIds: images,
    mImageCarouselObjectIds: images,
  });
});

test("imported games with no images leave both arrays empty", () => {
  assert.deepEqual(importedImageFields([]), {
    mImageLibraryObjectIds: [],
    mImageCarouselObjectIds: [],
  });
});

test("deleting a library image also removes it from the carousel", () => {
  assert.deepEqual(
    removeImageFromCarousel(
      ["screenshot-1", "screenshot-2", "screenshot-2", "screenshot-3"],
      "screenshot-2",
    ),
    ["screenshot-1", "screenshot-3"],
  );
});

test("deleting an image that is not in the carousel leaves it unchanged", () => {
  assert.deepEqual(
    removeImageFromCarousel(["screenshot-1", "screenshot-2"], "screenshot-3"),
    ["screenshot-1", "screenshot-2"],
  );
});
