import { type } from "arktype";
import type { Prisma } from "~/prisma/client/client";

export const emulationFilterSchema = type(
  "'true' | 'false' | 'true,false' | 'false,true'",
);

export function getEmulationFilter(
  value: typeof emulationFilterSchema.infer | undefined,
): Prisma.GameWhereInput {
  if (value === "true") return { isEmulated: true };
  if (value === "false") return { isEmulated: false };
  return {};
}
