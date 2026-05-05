FROM rust:1.94.1-slim-bookworm AS builder
WORKDIR /app

# 1. 替换 apt 源并安装构建依赖（增加 libpq-dev，供 sqlx-cli 编译使用）
RUN rm -f /etc/apt/sources.list.d/debian.sources \
    && echo "deb [trusted=yes] http://mirrors.aliyun.com/debian  bookworm main" > /etc/apt/sources.list \
    && echo "deb [trusted=yes] http://mirrors.aliyun.com/debian  bookworm-updates main" >> /etc/apt/sources.list \
    && echo "deb [trusted=yes] http://mirrors.aliyun.com/debian-security  bookworm-security main" >> /etc/apt/sources.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# 2. 替换 Cargo 源（修正为 printf，确保换行符生效）
RUN mkdir -p .cargo \
    && printf '[source.crates-io]\nreplace-with = "rsproxy-sparse"\n[source.rsproxy-sparse]\nregistry = "sparse+https://rsproxy.cn/index/"\n[net]\ngit-fetch-with-cli = true\n' > .cargo/config.toml

# 3. 安装 sqlx-cli（运行时执行迁移用）
RUN cargo install sqlx-cli --no-default-features --features native-tls,postgres

# 4. 依赖缓存层
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && printf 'fn main() {}' > src/main.rs \
    && cargo build --locked --release \
    && rm -rf src target/release/deps/qxnzy_microcourse_backend-* target/release/qxnzy-microcourse-backend target/release/incremental 2>/dev/null || true

# 5. 拷贝源码、sqlx 离线查询数据和迁移文件
COPY src ./src
COPY .sqlx ./.sqlx
COPY migrations ./migrations

# 6. 离线模式编译（构建阶段无法保证数据库可达，这是标准做法）
ENV SQLX_OFFLINE=true
RUN cargo build --locked --release

# --- 运行阶段 ---
FROM debian:bookworm-slim AS runtime
RUN rm -f /etc/apt/sources.list.d/debian.sources \
    && echo "deb [trusted=yes] http://mirrors.aliyun.com/debian  bookworm main" > /etc/apt/sources.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 libpq5 postgresql-client ffmpeg \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --shell /usr/sbin/nologin --uid 10001 app

WORKDIR /app

# 复制应用二进制、sqlx-cli 工具、迁移文件和启动脚本
COPY --from=builder --chown=app:app /app/target/release/qxnzy-microcourse-backend ./
COPY --from=builder /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
COPY --from=builder --chown=app:app /app/migrations ./migrations
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

USER app
EXPOSE 8080
ENV RUST_LOG=info
ENTRYPOINT ["/entrypoint.sh"]
CMD ["./qxnzy-microcourse-backend"]