const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];
const HEADER_LIMIT = 256;
const generatedCovers = new Map<string, Promise<boolean>>();

export function isGeneratedArtworkHeader(header: Uint8Array): boolean {
  if (!PNG_SIGNATURE.every((byte, index) => header[index] === byte))
    return false;
  const view = new DataView(
    header.buffer,
    header.byteOffset,
    header.byteLength,
  );
  const decoder = new TextDecoder();
  let offset = PNG_SIGNATURE.length;
  while (offset + 12 <= header.length) {
    const length = view.getUint32(offset);
    const end = offset + 12 + length;
    if (end > header.length) return false;
    const kind = decoder.decode(header.subarray(offset + 4, offset + 8));
    if (
      kind === "tEXt" &&
      decoder.decode(header.subarray(offset + 8, end - 4)) ===
        "Software\0Jdenticon"
    )
      return true;
    offset = end;
  }
  return false;
}

export function isGeneratedCover(src: string): Promise<boolean> {
  const existing = generatedCovers.get(src);
  if (existing) return existing;

  const check = (async () => {
    // The image has already loaded; reuse its immutable browser-cache entry.
    const response = await fetch(src, { cache: "force-cache" });
    if (!response.ok || !response.body) throw new Error("Cover unavailable");
    const reader = response.body.getReader();
    const header = new Uint8Array(HEADER_LIMIT);
    let length = 0;
    try {
      while (length < HEADER_LIMIT) {
        const chunk = await reader.read();
        if (chunk.done) break;
        const bytes = chunk.value.subarray(0, HEADER_LIMIT - length);
        header.set(bytes, length);
        length += bytes.length;
      }
      return isGeneratedArtworkHeader(header.subarray(0, length));
    } finally {
      await reader.cancel();
    }
  })().catch(() => {
    generatedCovers.delete(src);
    return false;
  });

  // Keep long browsing sessions bounded. Object URLs are immutable.
  if (generatedCovers.size >= 256)
    generatedCovers.delete(generatedCovers.keys().next().value!);
  generatedCovers.set(src, check);
  return check;
}
