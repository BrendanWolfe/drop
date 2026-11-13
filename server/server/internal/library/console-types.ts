import { type } from "arktype";

export function normalizeConsoleName(name: string) {
  return name.normalize("NFKC").trim().replace(/\s+/gu, " ");
}

export const consoleNameSchema = type("string <= 200")
  .narrow((name) => !/\p{Cc}/u.test(name))
  .pipe(normalizeConsoleName)
  .to("0 < string <= 80");

export function consoleNameKey(name: string) {
  return normalizeConsoleName(name).toLowerCase();
}

export const consoleTypeIdSchema = type("string.uuid | null");
