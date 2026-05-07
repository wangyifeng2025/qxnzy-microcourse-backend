-- 为答题记录表添加每题得分 JSONB 列
-- 格式: {"<question_uuid>": 5.0, "<question_uuid>": null}
-- null 表示该题（问答题）尚未由教师评分
ALTER TABLE quiz_attempts
    ADD COLUMN IF NOT EXISTS question_scores JSONB;
