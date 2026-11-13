import { type } from "arktype";
import type { Prisma } from "~/prisma/client/client";

export const emulationFilterSchema = type(
  "'true' | 'false' | 'true,false' | 'false,true'",
);

export const nameSearchSchema = type("string <= 200");
const consoleIdSchema = type("string.uuid");
export const consoleFilterSchema = type("0 < string <= 3699").narrow(
  (value) => {
    const ids = value.split(",");
    return ids.length <= 100 && ids.every((id) => consoleIdSchema.allows(id));
  },
);

export function combineStoreFilters(
  ...filters: Array<Prisma.GameWhereInput | undefined>
): Prisma.GameWhereInput {
  // Keep each restriction separate: platform, company and console filters use OR,
  // while age restrictions use AND. Spreading them would overwrite restrictions.
  return {
    type: "Game",
    AND: filters.filter((filter) => filter !== undefined),
  };
}

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
  consoles?: string,
): Prisma.GameWhereInput {
  if (value === "false") return { isEmulated: false };
  const ids = [...new Set(consoles?.split(",").filter(Boolean))];
  if (ids.length) {
    const emulated = { isEmulated: true, consoleTypeId: { in: ids } };
    return value?.includes("false")
      ? { OR: [{ isEmulated: false }, emulated] }
      : emulated;
  }
  if (value === "true") return { isEmulated: true };
  return {};
}
