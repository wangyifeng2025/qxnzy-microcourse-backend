# qxnzy-microcourse 微课平台项目规划

## 1. 项目概述

### 1.1 项目背景

qxnzy-microcourse 是一个面向高职院校的微课学习平台，类似于中国大学慕课网/Coursera，提供校内课程视频学习服务。平台支持教师按专业分类上传教学视频，学生在线学习并跟踪学习进度，同时支持视频中的问答交互和章节测试作业功能。

### 1.2 项目目标

- **短期目标**：构建基础的课程视频学习平台，支持视频上传、播放、进度跟踪
- **中期目标**：增加问答交互、章节测试、成绩统计功能
- **长期目标**：接入大模型，实现虚拟教师、AI辅助学习等智能化功能

### 1.3 技术栈


| 层级        | 技术选型                     |
| --------- | ------------------------ |
| 后端框架      | Rust + Axum              |
| ORM/数据库访问 | SQLx                     |
| 数据库       | PostgreSQL               |
| 缓存        | Redis                    |
| 对象存储      | MinIO                    |
| 视频处理      | FFmpeg                   |
| 前端框架      | Next.js 14 + TypeScript  |
| 前端样式      | Tailwind CSS + shadcn/ui |
| 移动端       | Expo (React Native)      |
| 消息队列      | RabbitMQ / Kafka (后续)    |


---

## 2. 需求分析

### 2.1 用户角色

#### 2.1.1 学生 (Student)

- 浏览课程列表，按专业/分类筛选
- 观看教学视频，自动记录学习进度
- 参与视频中的问答交互
- 完成章节测试作业
- 查看学习进度和成绩统计

#### 2.1.2 教师 (Teacher)

- 创建和管理课程
- 上传教学视频（支持批量上传）
- 在视频中设置问答交互点
- 创建章节测试题目
- 查看学生学习进度和成绩
- 管理课程章节结构

#### 2.1.3 管理员 (Admin)

- 用户管理（学生/教师账号）
- 专业/分类管理
- 课程审核
- 系统配置
- 数据统计报表

### 2.2 功能模块

#### 2.2.1 用户认证模块 (Auth)


| 功能     | 描述           | 优先级 |
| ------ | ------------ | --- |
| 登录/注册  | 支持学号/工号+密码登录 | P0  |
| JWT认证  | 无状态认证机制      | P0  |
| 角色权限   | RBAC权限控制     | P0  |
| 个人信息管理 | 修改密码、头像等     | P1  |


#### 2.2.2 课程管理模块 (Course)


| 功能     | 描述         | 优先级 |
| ------ | ---------- | --- |
| 课程CRUD | 创建、编辑、删除课程 | P0  |
| 专业分类   | 按专业/学科分类管理 | P0  |
| 章节管理   | 课程的章节结构管理  | P0  |
| 课程封面   | 封面上传和展示    | P1  |
| 课程搜索   | 关键词搜索和筛选   | P1  |


#### 2.2.3 视频管理模块 (Video)


| 功能   | 描述            | 优先级 |
| ---- | ------------- | --- |
| 视频上传 | 支持大文件分片上传     | P0  |
| 视频转码 | FFmpeg转码为多清晰度 | P0  |
| 视频截图 | 自动生成封面和预览图    | P0  |
| 视频播放 | 支持多清晰度切换      | P0  |
| 进度记录 | 自动保存观看进度      | P0  |
| 问答交互 | 视频中插入问答点      | P1  |


#### 2.2.4 学习进度模块 (Progress)


| 功能   | 描述          | 优先级 |
| ---- | ----------- | --- |
| 进度记录 | 记录每个视频的观看进度 | P0  |
| 学习统计 | 学习时长、完成率统计  | P1  |
| 最近学习 | 继续上次学习      | P1  |


#### 2.2.5 测试作业模块 (Quiz)


| 功能   | 描述        | 优先级 |
| ---- | --------- | --- |
| 题目管理 | 单选、多选、判断题 | P1  |
| 试卷组卷 | 按章节组卷     | P1  |
| 在线答题 | 限时/不限时答题  | P1  |
| 自动评分 | 客观题自动评分   | P1  |
| 成绩统计 | 成绩记录和统计   | P1  |


#### 2.2.6 问答交互模块 (Interaction)


| 功能    | 描述          | 优先级 |
| ----- | ----------- | --- |
| 问答点设置 | 教师在视频中设置问题  | P2  |
| 弹题播放  | 播放到指定位置弹出题目 | P2  |
| 答题反馈  | 即时反馈对错      | P2  |


### 2.3 非功能需求

#### 2.3.1 性能需求

- 视频首屏加载时间 < 3秒
- API响应时间 < 200ms (P95)
- 支持1000+并发用户

#### 2.3.2 安全需求

- 视频防盗链（URL签名）
- SQL注入防护（SQLx参数化查询）
- XSS防护
- 敏感数据加密存储

#### 2.3.3 可用性需求

- 系统可用性 99.9%
- 数据定期备份
- 故障恢复机制

---

## 3. 数据库设计概要

### 3.1 核心表结构

```sql
-- 用户表
users (id, username, email, password_hash, role, avatar, created_at, updated_at)

-- 专业分类表
majors (id, name, code, description, parent_id, created_at)

-- 课程表
courses (id, title, description, cover_image, major_id, teacher_id, status, created_at, updated_at)

-- 章节表
chapters (id, course_id, title, sort_order, created_at)

-- 视频表
videos (id, chapter_id, title, description, duration, video_url, cover_url, status, created_at)

-- 视频转码记录
video_transcodes (id, video_id, resolution, url, status, created_at)

-- 学习进度表
learning_progress (id, user_id, video_id, progress, last_position, completed_at, updated_at)

-- 问答表
questions (id, video_id, position_seconds, question_type, content, options, answer, created_at)

-- 测试表
quizzes (id, course_id, chapter_id, title, time_limit, total_score, created_at)

-- 题目表
quiz_questions (id, quiz_id, question_type, content, options, answer, score, sort_order)

-- 答题记录表
quiz_attempts (id, user_id, quiz_id, score, answers, started_at, submitted_at)

-- 问答交互记录
question_responses (id, user_id, question_id, answer, is_correct, responded_at)
```

---

## 4. 项目目录结构

### 4.1 后端项目 (qxnzy-microcourse-backend)

```
qxnzy-microcourse-backend/
├── Cargo.toml                    # 项目配置
├── Cargo.lock
├── .env                          # 环境变量
├── .env.example                  # 环境变量示例
├── Dockerfile                    # Docker构建
├── docker-compose.yml            # 本地开发环境
├── migrations/                   # 数据库迁移文件
│   ├── 001_init_users.sql
│   ├── 002_init_courses.sql
│   ├── 003_init_videos.sql
│   └── ...
├── src/
│   ├── main.rs                   # 程序入口
│   ├── config.rs                 # 配置管理
│   ├── lib.rs                    # 库入口
│   ├── error.rs                  # 错误处理
│   ├── state.rs                  # AppState
│   ├── router.rs                 # 路由聚合
│   ├── middleware/               # 中间件
│   │   ├── mod.rs
│   │   ├── auth.rs               # 认证中间件
│   │   ├── cors.rs               # CORS中间件
│   │   └── logging.rs            # 日志中间件
│   ├── handlers/                 # 请求处理器
│   │   ├── mod.rs
│   │   ├── auth.rs               # 认证相关
│   │   ├── user.rs               # 用户管理
│   │   ├── course.rs             # 课程管理
│   │   ├── chapter.rs            # 章节管理
│   │   ├── video.rs              # 视频管理
│   │   ├── progress.rs           # 学习进度
│   │   ├── quiz.rs               # 测试作业
│   │   ├── question.rs           # 问答交互
│   │   ├── upload.rs             # 文件上传
│   │   └── admin.rs              # 管理后台
│   ├── services/                 # 业务逻辑层
│   │   ├── mod.rs
│   │   ├── auth_service.rs
│   │   ├── user_service.rs
│   │   ├── course_service.rs
│   │   ├── video_service.rs
│   │   ├── upload_service.rs
│   │   ├── transcode_service.rs  # 视频转码
│   │   ├── progress_service.rs
│   │   ├── quiz_service.rs
│   │   └── notification_service.rs
│   ├── models/                   # 数据模型
│   │   ├── mod.rs
│   │   ├── user.rs
│   │   ├── course.rs
│   │   ├── video.rs
│   │   ├── progress.rs
│   │   ├── quiz.rs
│   │   └── common.rs             # 通用类型
│   ├── db/                       # 数据库相关
│   │   ├── mod.rs
│   │   ├── pool.rs               # 连接池
│   │   └── queries/              # SQL查询
│   │       ├── user.sql
│   │       ├── course.sql
│   │       └── ...
│   ├── utils/                    # 工具函数
│   │   ├── mod.rs
│   │   ├── jwt.rs                # JWT工具
│   │   ├── hash.rs               # 密码哈希
│   │   ├── validation.rs         # 参数校验
│   │   ├── response.rs           # 响应封装
│   │   └── minio.rs              # MinIO客户端
│   └── tasks/                    # 后台任务
│       ├── mod.rs
│       └── transcode_worker.rs   # 转码工作进程
├── tests/                        # 集成测试
│   ├── integration_tests.rs
│   └── test_data.sql
└── scripts/                      # 脚本
    ├── dev.sh                    # 开发启动
    ├── migrate.sh                # 数据库迁移
    └── seed.sh                   # 数据种子
```

### 4.2 前端项目 (qxnzy-microcourse-frontend)

```
qxnzy-microcourse-frontend/
├── package.json
├── package-lock.json
├── next.config.js
├── tailwind.config.ts
├── tsconfig.json
├── .env.local
├── .env.example
├── Dockerfile
├── public/
│   ├── images/
│   ├── icons/
│   └── fonts/
├── src/
│   ├── app/                      # Next.js App Router
│   │   ├── layout.tsx            # 根布局
│   │   ├── page.tsx              # 首页
│   │   ├── globals.css
│   │   ├── (auth)/               # 认证相关页面
│   │   │   ├── login/
│   │   │   ├── register/
│   │   │   └── forgot-password/
│   │   ├── (student)/            # 学生端页面
│   │   │   ├── courses/
│   │   │   │   ├── page.tsx      # 课程列表
│   │   │   │   └── [id]/
│   │   │   │       ├── page.tsx  # 课程详情
│   │   │   │       └── learn/
│   │   │   │           └── page.tsx  # 学习页面
│   │   │   ├── my-courses/
│   │   │   ├── progress/
│   │   │   └── quizzes/
│   │   ├── (teacher)/            # 教师端页面
│   │   │   ├── dashboard/
│   │   │   ├── my-courses/
│   │   │   ├── course-editor/
│   │   │   ├── video-upload/
│   │   │   ├── quiz-editor/
│   │   │   └── students/
│   │   └── admin/                # 管理后台
│   │       ├── dashboard/
│   │       ├── users/
│   │       ├── courses/
│   │       ├── majors/
│   │       └── settings/
│   ├── components/               # 组件
│   │   ├── ui/                   # shadcn/ui 基础组件
│   │   ├── common/               # 通用组件
│   │   │   ├── Header.tsx
│   │   │   ├── Sidebar.tsx
│   │   │   ├── Footer.tsx
│   │   │   ├── Loading.tsx
│   │   │   └── ErrorBoundary.tsx
│   │   ├── video/                # 视频相关组件
│   │   │   ├── VideoPlayer.tsx
│   │   │   ├── VideoList.tsx
│   │   │   ├── ProgressBar.tsx
│   │   │   └── QuestionModal.tsx
│   │   ├── course/               # 课程相关组件
│   │   │   ├── CourseCard.tsx
│   │   │   ├── CourseList.tsx
│   │   │   ├── ChapterTree.tsx
│   │   │   └── CourseForm.tsx
│   │   ├── quiz/                 # 测试相关组件
│   │   │   ├── QuizCard.tsx
│   │   │   ├── QuestionForm.tsx
│   │   │   ├── QuizPlayer.tsx
│   │   │   └── ResultView.tsx
│   │   └── upload/               # 上传组件
│   │       ├── UploadButton.tsx
│   │       ├── UploadProgress.tsx
│   │       └── DragDropZone.tsx
│   ├── hooks/                    # 自定义Hooks
│   │   ├── useAuth.ts
│   │   ├── useCourse.ts
│   │   ├── useVideo.ts
│   │   ├── useProgress.ts
│   │   ├── useUpload.ts
│   │   └── useQuiz.ts
│   ├── lib/                      # 工具库
│   │   ├── api.ts                # API客户端
│   │   ├── auth.ts               # 认证工具
│   │   ├── utils.ts              # 通用工具
│   │   └── constants.ts          # 常量
│   ├── types/                    # TypeScript类型
│   │   ├── user.ts
│   │   ├── course.ts
│   │   ├── video.ts
│   │   ├── quiz.ts
│   │   └── api.ts
│   ├── store/                    # 状态管理 (Zustand)
│   │   ├── authStore.ts
│   │   ├── courseStore.ts
│   │   └── uiStore.ts
│   └── styles/                   # 样式文件
│       └── variables.css
├── components.json               # shadcn/ui 配置
└── scripts/
    └── setup.sh
```

### 4.3 移动端项目 (qxnzy-microcourse-mobile)

```
qxnzy-microcourse-mobile/
├── package.json
├── app.json                      # Expo配置
├── tsconfig.json
├── .env
├── assets/                       # 静态资源
│   ├── images/
│   ├── fonts/
│   └── icons/
├── src/
│   ├── app/                      # Expo Router
│   │   ├── _layout.tsx           # 根布局
│   │   ├── index.tsx             # 首页
│   │   ├── (tabs)/               # 底部导航
│   │   │   ├── _layout.tsx
│   │   │   ├── home.tsx
│   │   │   ├── courses.tsx
│   │   │   ├── my-learning.tsx
│   │   │   └── profile.tsx
│   │   ├── (auth)/               # 认证
│   │   │   ├── login.tsx
│   │   │   └── register.tsx
│   │   ├── course/
│   │   │   └── [id].tsx          # 课程详情
│   │   ├── learn/
│   │   │   └── [videoId].tsx     # 学习页面
│   │   └── quiz/
│   │       └── [id].tsx          # 测试页面
│   ├── components/               # 组件
│   │   ├── common/
│   │   │   ├── Header.tsx
│   │   │   ├── Loading.tsx
│   │   │   └── ErrorView.tsx
│   │   ├── course/
│   │   │   ├── CourseCard.tsx
│   │   │   └── CourseList.tsx
│   │   └── video/
│   │       ├── VideoPlayer.tsx
│   │       └── Controls.tsx
│   ├── hooks/
│   │   ├── useAuth.ts
│   │   ├── useApi.ts
│   │   └── useVideo.ts
│   ├── services/
│   │   ├── api.ts
│   │   ├── auth.ts
│   │   ├── course.ts
│   │   └── video.ts
│   ├── types/
│   │   └── index.ts
│   ├── store/
│   │   └── authStore.ts
│   └── utils/
│       └── constants.ts
└── scripts/
    └── setup.sh
```

---

## 5. API设计概要

### 5.1 RESTful API 设计

```
# 认证
POST   /api/v1/auth/login
POST   /api/v1/auth/register
POST   /api/v1/auth/refresh
POST   /api/v1/auth/logout

# 用户
GET    /api/v1/users/me
PUT    /api/v1/users/me
GET    /api/v1/users/:id

# 课程
GET    /api/v1/courses              # 列表（支持筛选）
POST   /api/v1/courses              # 创建（教师）
GET    /api/v1/courses/:id
PUT    /api/v1/courses/:id          # 更新（教师）
DELETE /api/v1/courses/:id          # 删除（教师）
GET    /api/v1/courses/:id/chapters

# 章节
POST   /api/v1/chapters
PUT    /api/v1/chapters/:id
DELETE /api/v1/chapters/:id

# 视频
GET    /api/v1/videos/:id
POST   /api/v1/videos               # 创建（教师）
PUT    /api/v1/videos/:id
DELETE /api/v1/videos/:id
POST   /api/v1/videos/:id/upload    # 上传视频
GET    /api/v1/videos/:id/stream    # 获取播放地址
GET    /api/v1/videos/:id/questions # 获取问答点

# 学习进度
GET    /api/v1/progress
GET    /api/v1/progress/course/:id
POST   /api/v1/progress             # 更新进度

# 测试
GET    /api/v1/quizzes
POST   /api/v1/quizzes              # 创建（教师）
GET    /api/v1/quizzes/:id
POST   /api/v1/quizzes/:id/submit   # 提交答案
GET    /api/v1/quizzes/:id/result   # 查看结果

# 问答交互
POST   /api/v1/questions            # 创建问题（教师）
POST   /api/v1/questions/:id/answer # 回答问题

# 上传
POST   /api/v1/upload/presign       # 获取预签名URL
POST   /api/v1/upload/callback      # 上传回调

# 管理后台
GET    /api/v1/admin/users
GET    /api/v1/admin/stats
GET    /api/v1/admin/majors
```

---

## 6. 开发里程碑

### 阶段一：基础架构搭建 (Week 1-2)

**目标**：搭建项目基础架构，完成用户认证


| 任务  | 描述                            | 负责人      |
| --- | ----------------------------- | -------- |
| 1.1 | ~~后端项目初始化，配置Axum + SQLx~~     | Backend  |
| 1.2 | ~~数据库设计，编写迁移脚本~~              | Backend  |
| 1.3 | ~~用户认证API（注册/登录/JWT）~~        | Backend  |
| 1.4 | 前端项目初始化，配置Next.js + shadcn/ui | Frontend |
| 1.5 | 登录/注册页面                       | Frontend |
| 1.6 | Docker Compose开发环境            | DevOps   |


**交付物**：

- 可运行的登录/注册功能
- 基础项目架构

---

### 阶段二：课程与视频管理 (Week 3-4)

**目标**：实现课程和视频的基础CRUD功能


| 任务  | 描述              | 负责人      |
| --- | --------------- | -------- |
| 2.1 | ~~专业分类管理API~~   | Backend  |
| 2.2 | ~~课程CRUD API~~  | Backend  |
| 2.3 | ~~章节管理API~~     | Backend  |
| 2.4 | 视频元数据管理API      | Backend  |
| 2.5 | MinIO集成，文件上传API | Backend  |
| 2.6 | 课程列表/详情页面       | Frontend |
| 2.7 | 教师课程管理页面        | Frontend |
| 2.8 | 视频上传组件          | Frontend |


**交付物**：

- 教师可创建课程和上传视频
- 学生可浏览课程列表

---

### 阶段三：视频播放与转码 (Week 5-6)

**目标**：实现视频播放和转码功能


| 任务  | 描述                  | 负责人      |
| --- | ------------------- | -------- |
| 3.1 | FFmpeg转码服务          | Backend  |
| 3.2 | 视频转码任务队列            | Backend  |
| 3.3 | 多清晰度视频管理            | Backend  |
| 3.4 | 视频流播放API（支持Range请求） | Backend  |
| 3.5 | 视频播放器组件（支持清晰度切换）    | Frontend |
| 3.6 | 视频封面生成              | Backend  |


**交付物**：

- 上传的视频自动转码
- 学生可流畅观看视频

---

### 阶段四：学习进度 (Week 7)

**目标**：实现学习进度跟踪


| 任务  | 描述        | 负责人      |
| --- | --------- | -------- |
| 4.1 | 学习进度记录API | Backend  |
| 4.2 | 进度查询API   | Backend  |
| 4.3 | 视频播放进度同步  | Frontend |
| 4.4 | "继续学习"功能  | Frontend |
| 4.5 | 学习统计页面    | Frontend |


**交付物**：

- 自动记录观看进度
- 可查看学习统计

---

### 阶段五：测试作业 (Week 8-9)

**目标**：实现章节测试功能


| 任务  | 描述         | 负责人      |
| --- | ---------- | -------- |
| 5.1 | 题目管理API    | Backend  |
| 5.2 | 试卷组卷API    | Backend  |
| 5.3 | 答题提交和评分API | Backend  |
| 5.4 | 题目编辑器（教师端） | Frontend |
| 5.5 | 在线答题页面     | Frontend |
| 5.6 | 成绩查看页面     | Frontend |


**交付物**：

- 教师可创建测试
- 学生可在线答题并查看成绩

---

### 阶段六：问答交互 (Week 10)

**目标**：实现视频中的问答交互


| 任务  | 描述        | 负责人      |
| --- | --------- | -------- |
| 6.1 | 问答点管理API  | Backend  |
| 6.2 | 问答答题记录API | Backend  |
| 6.3 | 视频问答点编辑器  | Frontend |
| 6.4 | 播放时弹题组件   | Frontend |


**交付物**：

- 教师可在视频中设置问题
- 学生观看时弹出题目

---

### 阶段七：移动端开发 (Week 11-12)

**目标**：开发移动端应用


| 任务  | 描述        | 负责人    |
| --- | --------- | ------ |
| 7.1 | Expo项目初始化 | Mobile |
| 7.2 | 移动端登录/注册  | Mobile |
| 7.3 | 课程浏览页面    | Mobile |
| 7.4 | 视频播放页面    | Mobile |
| 7.5 | 学习进度同步    | Mobile |
| 7.6 | 测试答题功能    | Mobile |


**交付物**：

- iOS/Android应用基础版本

---

### 阶段八：优化与部署 (Week 13-14)

**目标**：性能优化和生产环境部署


| 任务  | 描述          | 负责人     |
| --- | ----------- | ------- |
| 8.1 | API性能优化     | Backend |
| 8.2 | 视频CDN配置     | DevOps  |
| 8.3 | 缓存优化（Redis） | Backend |
| 8.4 | 生产环境部署脚本    | DevOps  |
| 8.5 | 监控和日志       | DevOps  |
| 8.6 | 文档完善        | All     |


**交付物**：

- 生产环境可用的系统
- 部署文档

---

### 阶段九：AI功能（后续规划）

**目标**：接入大模型，实现智能化功能


| 任务  | 描述     | 优先级 |
| --- | ------ | --- |
| 9.1 | 虚拟教师对话 | P2  |
| 9.2 | AI辅助出题 | P2  |
| 9.3 | 智能学习推荐 | P3  |
| 9.4 | 视频内容摘要 | P3  |
| 9.5 | 学习行为分析 | P3  |


---

## 7. 技术要点与注意事项

### 7.1 视频处理

- 使用FFmpeg进行视频转码，生成多清晰度版本（1080p, 720p, 480p）
- 使用HLS/DASH协议进行流媒体传输
- 视频存储在MinIO，支持预签名URL防盗链
- 转码任务异步处理，使用消息队列

### 7.2 大文件上传

- 前端使用分片上传，支持断点续传
- 后端使用MinIO的multipart upload
- 上传进度实时反馈

### 7.3 性能优化

- 视频使用CDN加速
- API响应缓存（Redis）
- 数据库查询优化，添加合适索引
- 图片/视频压缩处理

### 7.4 安全考虑

- JWT Token设置合理过期时间
- 敏感操作需要二次验证
- 视频URL添加过期时间签名
- 文件上传类型和大小限制

---

## 8. 开发规范

### 8.1 代码规范

- Rust: 使用 `cargo clippy` 和 `cargo fmt`
- TypeScript: 使用 ESLint + Prettier
- 提交信息遵循 Conventional Commits

### 8.2 分支管理

```
main        # 生产分支
develop     # 开发分支
feature/*   # 功能分支
hotfix/*    # 紧急修复
```

### 8.3 版本号规则

遵循 Semantic Versioning: `MAJOR.MINOR.PATCH`

---

## 9. 附录

### 9.1 依赖库参考

**后端 (Cargo.toml)**

```toml
[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
jsonwebtoken = "9"
bcrypt = "0.15"
validator = "0.16"
tower = "0.4"
tower-http = "0.5"
tracing = "0.1"
tracing-subscriber = "0.3"
config = "0.14"
uuid = { version = "1.6", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
rust-s3 = "0.33"  # MinIO客户端
reqwest = { version = "0.11", features = ["json"] }
```

**前端 (package.json)**

```json
{
  "dependencies": {
    "next": "14",
    "react": "^18",
    "react-dom": "^18",
    "@tanstack/react-query": "^5",
    "axios": "^1.6",
    "zustand": "^4.4",
    "zod": "^3.22",
    "react-hook-form": "^7.48",
    "@hookform/resolvers": "^3.3",
    "video.js": "^8.6",
    "dayjs": "^1.11"
  }
}
```

---

*文档版本: v1.0*
*最后更新: 2024年*