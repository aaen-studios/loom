# loom's server in a container (PLAN.md, Phase 10). The instance keeps
# everything in /data, a volume, and listens on 8790. TLS comes from whatever
# is in front of it, as on a server.
#
#     docker build -t loom .
#     docker run -d --name loom -p 127.0.0.1:8790:8790 -v loom-data:/data loom
#     docker logs loom    # the one-time setup code, on the first start
#
# Commands go to the same program: docker exec loom loom user list --data /data

# The build: Calx, with the toolchain it brings, and loom's server.
FROM debian:bookworm-slim AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl xz-utils libc6-dev libcurl4 \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://calxlang.com/install.sh | sh
ENV PATH=/root/.calx/bin:$PATH \
    CALX_NO_UPDATE_CHECK=1 \
    NO_COLOR=1
WORKDIR /src
COPY shared shared
COPY server server
ARG VERSION=0.0.0-docker
RUN sed -i "s/^const VERSION: str = \".*\"/const VERSION: str = \"$VERSION\"/" server/main.calx \
    && grep -q "const VERSION: str = \"$VERSION\"" server/main.calx \
    && calx build --release server/main.calx -o /out/loom

# What runs: Node, for the ACP agents most people use (they start with npx),
# libcurl, which std.http uses on Linux, and git for agents that work in repos.
FROM node:22-bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libcurl4 git \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir /data \
    && chown node:node /data
COPY --from=build /out/loom /usr/local/bin/loom
# The image is updated by pulling it, so the instance doesn't update itself here.
ENV LOOM_SERVICE=docker
# The image's own unprivileged user, whose rights its agents get.
USER node
VOLUME /data
EXPOSE 8790
ENTRYPOINT ["loom"]
CMD ["serve", "--data", "/data", "--host", "0.0.0.0", "--port", "8790"]
