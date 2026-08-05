FROM rust:1-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --locked --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl gosu \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 adelia \
    && useradd --system --uid 10001 --gid adelia --home-dir /opt/adelia adelia

WORKDIR /opt/adelia
COPY --from=builder /build/target/release/adelia /usr/local/bin/adelia
COPY app_templates ./app_templates
COPY web ./web
COPY scripts/docker-entrypoint.sh /usr/local/bin/adelia-entrypoint

RUN chmod 0755 /usr/local/bin/adelia /usr/local/bin/adelia-entrypoint \
    && mkdir -p generated data/uploads \
    && chown -R adelia:adelia /opt/adelia

ENV GENERATED_DIR=generated \
    UPLOAD_DIR=data/uploads \
    TEMPLATE_DIR=app_templates \
    ASSET_DIR=web/assets

EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s --retries=3 \
    CMD curl --fail --silent --show-error http://127.0.0.1:8080/healthz || exit 1

ENTRYPOINT ["adelia-entrypoint"]
CMD ["adelia", "serve"]
