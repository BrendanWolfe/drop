import prisma from "../internal/db/database.ts";

export default defineNitroPlugin(async () => {
  // Resolve against each client's configured server so LAN and proxy connections use their own origin.
  const torrentialUrl = "/api/v1/depot/";

  await prisma.depot.upsert({
    where: {
      id: "torrential",
    },
    update: {
      endpoint: torrentialUrl,
    },
    create: {
      id: "torrential",
      endpoint: torrentialUrl,
    },
  });
});
