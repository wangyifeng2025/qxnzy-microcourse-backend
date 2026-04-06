-- 移除 majors 表的 parent_id 列（专业为扁平分类，无层级）
ALTER TABLE majors DROP COLUMN IF EXISTS parent_id;
