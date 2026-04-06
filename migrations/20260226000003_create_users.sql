-- Active: 1771933319502@@127.0.0.1@5432@qxnzy_microcourse
CREATE TABLE users (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    username       VARCHAR(50) NOT NULL UNIQUE,   -- 登录名（学号/工号）
    email          VARCHAR(100) UNIQUE,            -- 邮箱（可选）
    password_hash  VARCHAR(255) NOT NULL,          -- bcrypt 哈希
    role           user_role   NOT NULL,
    real_name      VARCHAR(50),                    -- 真实姓名
    avatar_url     TEXT,                           -- 头像 MinIO URL
    is_active      BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
