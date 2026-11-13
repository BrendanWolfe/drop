import { type } from "arktype";
import type { Prisma } from "~/prisma/client/client";

export const emulationFilterSchema = type(
  "'true' | 'false' | 'true,false' | 'false,true'",
);

export const nameSearchSchema = type("string <= 200");

export function getNameFilter(
  value: string | undefined,
): Prisma.GameWhereInput {
  const name = value?.trim();
  if (!name) return {};
  // Prisma's contains uses LIKE: treat wildcard characters as literal title text.
  return {
    mName: {
      contains: name.replace(/[\\%_]/g, "\\$&"),
      mode: "insensitive",
    },
  };
}

export function getEmulationFilter(
  value: typeof emulationFilterSchema.infer | undefined,
): Prisma.GameWhereInput {
  if (value === "true") return { isEmulated: true };
  if (value === "false") return { isEmulated: false };
  return {};
}
