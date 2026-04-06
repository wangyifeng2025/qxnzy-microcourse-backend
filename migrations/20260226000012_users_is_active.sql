-- 与旧库或手工建表脚本同步：账号启用状态（默认启用）
ALTER TABLE users ADD COLUMN IF NOT EXISTS is_active BOOLEAN NOT NULL DEFAULT TRUE;
