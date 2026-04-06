-- 用户：登录查询
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email) WHERE email IS NOT NULL;

-- 课程：按专业、教师、状态筛选
CREATE INDEX idx_courses_major_id ON courses(major_id);
CREATE INDEX idx_courses_teacher_id ON courses(teacher_id);
CREATE INDEX idx_courses_status ON courses(status);

-- 章节：按课程排序
CREATE INDEX idx_chapters_course_sort ON chapters(course_id, sort_order);

-- 视频：按章节排序
CREATE INDEX idx_videos_chapter_sort ON videos(chapter_id, sort_order);

-- 转码：按视频和状态查询
CREATE INDEX idx_video_transcodes_video_status ON video_transcodes(video_id, status);

-- 学习进度：用户进度、学习统计（user_id + video_id 已有 UNIQUE 约束，PostgreSQL 会建唯一索引）
CREATE INDEX idx_learning_progress_user_completed ON learning_progress(user_id, is_completed);

-- 答题记录
CREATE INDEX idx_quiz_attempts_user_quiz ON quiz_attempts(user_id, quiz_id);

-- 视频问答点：按视频和触发时间排序
CREATE INDEX idx_video_questions_video_position ON video_questions(video_id, position_seconds);
