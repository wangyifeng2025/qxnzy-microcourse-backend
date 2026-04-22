-- 添加密码重置标记字段
-- 管理员重置用户密码后，该字段置为 true，提醒用户登录后自行修改密码
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS password_reset_required BOOLEAN NOT NULL DEFAULT FALSE;
