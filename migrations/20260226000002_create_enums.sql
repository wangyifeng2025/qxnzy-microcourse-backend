-- 用户角色：学生 / 教师 / 管理员
CREATE TYPE user_role AS ENUM ('student', 'teacher', 'admin');

-- 课程状态：草稿 / 已发布 / 已归档
CREATE TYPE course_status AS ENUM ('draft', 'published', 'archived');

-- 视频状态：待处理 / 转码中 / 就绪 / 失败
CREATE TYPE video_status AS ENUM ('pending', 'processing', 'ready', 'failed');

-- 转码任务状态：待处理 / 转码中 / 完成 / 失败
CREATE TYPE transcode_status AS ENUM ('pending', 'processing', 'done', 'failed');

-- 题目类型：单选 / 多选 / 判断
CREATE TYPE question_type AS ENUM ('single_choice', 'multiple_choice', 'true_false');
