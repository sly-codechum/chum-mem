FROM node:25-alpine AS base
WORKDIR /app
RUN rm -f /usr/local/bin/yarn /usr/local/bin/yarnpkg /usr/local/bin/pnpm /usr/local/bin/pnpx
RUN npm install -g corepack@latest
RUN corepack enable

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY apps ./apps
COPY packages ./packages
COPY infra ./infra

RUN pnpm install --frozen-lockfile
RUN pnpm build

FROM node:25-alpine AS runtime
WORKDIR /app

COPY --from=base /app/package.json /app/pnpm-lock.yaml /app/pnpm-workspace.yaml /app/tsconfig.base.json ./
COPY --from=base /app/node_modules ./node_modules
COPY --from=base /app/apps ./apps
COPY --from=base /app/packages ./packages
COPY --from=base /app/infra ./infra


FROM runtime AS web
EXPOSE 65300
CMD ["node", "apps/web/dist/index.js"]

