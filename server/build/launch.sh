#!/bin/sh
set -eu

# This file starts up the Drop server by running migrations and then starting the executable
schema=./prisma/schema.prisma
migrations=./prisma/migrations
test -f ./prisma.config.ts
test -f "$schema"
test -f "${NGINX_CONFIG:-/nginx.conf}"

count=0
for migration in "$migrations"/*/migration.sql; do
    if [ -f "$migration" ]; then
        count=$((count + 1))
    fi
done
if [ "$count" -eq 0 ]; then
    echo "[Drop] no migrations found in $PWD/prisma/migrations" >&2
    exit 1
fi

echo "[Drop] schema: $PWD/prisma/schema.prisma; migrations: $count"
echo "[Drop] performing migrations..."
pnpm exec prisma migrate deploy --config ./prisma.config.ts

# Actually start the application
exec node /app/app/server/index.mjs
