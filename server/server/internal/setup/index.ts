import type { UserWhereInput } from "~/prisma/client/models";

export const enabledSetupAdminWhere: UserWhereInput = {
  id: { not: "system" },
  admin: true,
  enabled: true,
  authMecs: {
    some: {
      enabled: true,
    },
  },
};

type SetupPrismaClient = {
  user: {
    count: (args: { where: UserWhereInput }) => Promise<number>;
  };
  aPIToken: {
    deleteMany: (args: {
      where: {
        mode: "System";
        acls: {
          hasSome: string[];
        };
      };
    }) => Promise<unknown>;
    create: (args: {
      data: {
        name: string;
        mode: "System";
        acls: string[];
      };
    }) => Promise<{ token: string }>;
  };
};

export async function ensureSetupToken(prisma: SetupPrismaClient) {
  const userCount = await prisma.user.count({
    where: { id: { not: "system" } },
  });
  if (userCount !== 0) return undefined;

  await prisma.aPIToken.deleteMany({
    where: {
      mode: "System",
      acls: {
        hasSome: ["setup"],
      },
    },
  });

  return await prisma.aPIToken.create({
    data: {
      name: "Setup Wizard",
      mode: "System",
      acls: ["setup"],
    },
  });
}

export async function hasEnabledSetupAdmin(prisma: SetupPrismaClient) {
  const count = await prisma.user.count({ where: enabledSetupAdminWhere });
  return count > 0;
}

export async function completeSetup(prisma: SetupPrismaClient) {
  if (!(await hasEnabledSetupAdmin(prisma))) return false;

  await prisma.aPIToken.deleteMany({
    where: {
      mode: "System",
      acls: {
        hasSome: ["setup"],
      },
    },
  });

  return true;
}
