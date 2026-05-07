-- ALTER TYPE ... ADD VALUE 在部分 PostgreSQL 版本中不能在事务内执行，故单独一个迁移文件
-- sqlx:disable-transaction
ALTER TYPE question_type ADD VALUE IF NOT EXISTS 'essay';
