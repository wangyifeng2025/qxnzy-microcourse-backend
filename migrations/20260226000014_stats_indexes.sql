-- 加速"专业→课程→报名/视频"聚合查询
CREATE INDEX IF NOT EXISTS idx_course_enrollments_course_id ON course_enrollments (course_id);

CREATE INDEX IF NOT EXISTS idx_videos_chapter_id ON videos (chapter_id);