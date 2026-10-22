import { micromark } from "micromark";

export function renderGameDescription(
  markdown: string,
  objectUrl: (id: string) => string,
): string {
  // Server-relative image URLs need the authenticated object protocol on desktop.
  return micromark(markdown).replace(
    /<img src="\/api\/v1\/object\/([^"/?#]+)"/g,
    (_match, id: string) => `<img src="${objectUrl(id)}"`,
  );
}
