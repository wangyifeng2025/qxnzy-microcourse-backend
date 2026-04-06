-- 学生所属专业（Teacher/Admin 通常为 NULL）
ALTER TABLE users ADD COLUMN IF NOT EXISTS major_id UUID REFERENCES majors(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_users_major_id ON users(major_id) WHERE major_id IS NOT NULL;
