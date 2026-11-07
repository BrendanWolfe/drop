-- Eligibility is explicit; do not reinterpret existing Linux-labelled launches.
ALTER TABLE "LaunchConfiguration" ADD COLUMN "protonCompatible" BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE "SetupConfiguration" ADD COLUMN "protonCompatible" BOOLEAN NOT NULL DEFAULT false;
