-- 启用 pgcrypto 扩展，提供 gen_random_uuid() 函数用于生成 UUID 主键
CREATE EXTENSION IF NOT EXISTS pgcrypto;
