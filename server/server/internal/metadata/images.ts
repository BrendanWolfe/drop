export function importedImageFields(imageLibraryObjectIds: readonly string[]) {
  return {
    mImageLibraryObjectIds: [...imageLibraryObjectIds],
    mImageCarouselObjectIds: [...imageLibraryObjectIds],
  };
}

export function removeImageFromCarousel(
  carouselObjectIds: readonly string[],
  imageId: string,
) {
  return carouselObjectIds.filter((id) => id !== imageId);
}
