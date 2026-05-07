
FROM rust:1.94.1-slim-bookworm AS builder
WORKDIR /app

# 1. 替换源为阿里云，绕过 GPG 密钥验证
RUN rm -f /etc/apt/sources.list.d/debian.sources \
    && echo "deb [trusted=yes] http://mirrors.aliyun.com/debian bookworm main" > /etc/apt/sources.list \
    && echo "deb [trusted=yes] http://mirrors.aliyun.com/debian bookworm-updates main" >> /etc/apt/sources.list \
    && echo "deb [trusted=yes] http://mirrors.aliyun.com/debian-security bookworm-security main" >> /etc/apt/sources.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# 2. 替换 Cargo 源为字节跳动
RUN mkdir -p .cargo \
    && echo '[source.crates-io]\nreplace-with = "rsproxy-sparse"\n[source.rsproxy-sparse]\nregistry = "sparse+https://rsproxy.cn/index/"\n[net]\ngit-fetch-with-cli = true' > .cargo/config.toml

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && printf 'fn main() {}' > src/main.rs \
    && cargo build --locked --release \
    && rm -rf src target/release/deps/qxnzy_microcourse_backend-* target/release/qxnzy-microcourse-backend target/release/incremental 2>/dev/null || true

# 3. 拷贝源码和 sqlx 离线数据
COPY src ./src
COPY .sqlx ./.sqlx

# 4. 强制开启 sqlx 离线模式，即使没有数据库也能编译通过！
ENV SQLX_OFFLINE=true
RUN cargo build --locked --release

# --- 运行阶段 ---
FROM debian:bookworm-slim AS runtime
RUN rm -f /etc/apt/sources.list.d/debian.sources \
    && echo "deb [trusted=yes] http://mirrors.aliyun.com/debian bookworm main" > /etc/apt/sources.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 ffmpeg \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --shell /usr/sbin/nologin --uid 10001 app

WORKDIR /app
COPY --from=builder --chown=app:app /app/target/release/qxnzy-microcourse-backend ./
USER app
EXPOSE 8080
ENV RUST_LOG=info
CMD ["./qxnzy-microcourse-backend"]