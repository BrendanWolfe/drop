import { type } from "arktype";
import { readDropValidatedBody, throwingArktype } from "~/server/arktype";
import aclManager from "~/server/internal/acls";
import prisma from "~/server/internal/db/database";
import {
  consoleNameKey,
  consoleNameSchema,
} from "~/server/internal/library/console-types";

const CreateConsole = type({ name: consoleNameSchema }).configure(
  throwingArktype,
);

export default defineEventHandler(async (h3) => {
  const allowed = await aclManager.allowSystemACL(h3, [
    "import:game:new",
    "game:update",
  ]);
  if (!allowed) throw createError({ statusCode: 403 });

  const { name } = await readDropValidatedBody(h3, CreateConsole);
  const nameKey = consoleNameKey(name);
  // A unique normalized key makes simultaneous and differently cased creates reusable.
  await prisma.consoleType.createMany({
    data: [{ name, nameKey }],
    skipDuplicates: true,
  });
  return prisma.consoleType.findUniqueOrThrow({
    where: { nameKey },
    select: { id: true, name: true },
  });
});
