-- Backfill carousel images for games imported before imports seeded the carousel.
-- Preserve existing carousels; only fill games that have library images but no carousel.
UPDATE "Game"
SET "mImageCarouselObjectIds" = "mImageLibraryObjectIds"
WHERE "mImageLibraryObjectIds" IS NOT NULL
  AND cardinality("mImageLibraryObjectIds") > 0
  AND (
    "mImageCarouselObjectIds" IS NULL
    OR cardinality("mImageCarouselObjectIds") = 0
  );
