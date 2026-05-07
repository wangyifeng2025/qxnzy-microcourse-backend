mod handlers;
mod middleware;
mod models;
mod repositories;
mod startup;
mod storage;
mod utils;
mod workers;

use std::sync::Arc;

use axum::{
    Extension, Router,
    middleware::{from_fn, from_fn_with_state},
    routing::{delete, get, post, put},
};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use handlers::auth::login;
use handlers::discover::{active_teachers, latest_topics, popular_courses};
use handlers::chapter::{
    create_chapter, delete_chapter, get_chapter, list_chapters, update_chapter,
};
use handlers::community::{
    create_reply, create_standalone_topic, create_standalone_topic_reply, create_topic,
    delete_reply, delete_standalone_topic, delete_standalone_topic_reply, delete_topic,
    get_standalone_topic, get_topic, join_standalone_topic, leave_standalone_topic,
    list_community_members, list_replies, list_standalone_topic_members,
    list_standalone_topic_replies, list_standalone_topics, list_topic_highlights, list_topics,
};
use handlers::course::{
    confirm_course_cover, create_course, delete_course, delete_course_cover, get_course,
    list_courses, list_courses_manage, request_course_cover_upload_url, toggle_course_vote,
    update_course,
};
use handlers::enrollment::{
    enroll_course, get_course_enrollment_count, get_enrollment_status, list_enrolled_courses,
    unenroll_course,
};
use handlers::major::{create_major, delete_major, get_major, list_majors, update_major};
use handlers::user::{
    admin_reset_password, change_password, create_user, delete_user, get_user, list_users,
    register_user, update_user,
};
use handlers::video::{
    confirm_upload, create_hls_url, create_video, delete_video, get_transcodes, get_video,
    hls_playlist, hls_segment, list_videos, request_upload_url, update_video,
};
use handlers::attempt::{
    start_attempt, submit_attempt, get_my_attempts, get_attempt_detail,
    list_all_attempts, grade_attempt,
};
use handlers::question::{
    // 题库管理
    list_questions, create_question, get_question, update_question, delete_question,
    // 题库导入/导出
    export_questions, import_questions,
    // 试卷管理
    list_exams, create_exam, get_exam, update_exam, delete_exam, toggle_exam_publish,
    // 试卷组题
    list_exam_questions, add_question_to_exam, update_question_in_exam, remove_question_from_exam,
};
use middleware::auth::{AllowedRoles, auth_middleware, require_roles_middleware};
use models::enums::UserRole;
use storage::AppStorage;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "qxnzy_microcourse_backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL 环境变量未设置");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("无法连接到数据库");

    tracing::info!("数据库连接成功");

    // 确保管理员账号存在（从环境变量读取，首次启动时写入）
    startup::seed_admin(&pool).await;

    let auth_layer = from_fn_with_state(pool.clone(), auth_middleware);

    // 初始化对象存储客户端（支持 MinIO / 阿里云 OSS / AWS S3）
    let storage: Arc<AppStorage> = AppStorage::from_env();

    // 确保存储桶可用（MinIO 自动创建；OSS 请提前在控制台建好 Bucket）
    if let Err(e) = storage.ensure_bucket().await {
        tracing::warn!(error = %e, "存储桶检查失败，请确认对象存储服务已启动且 Bucket 已创建");
    }

    // 启动转码 Worker（后台 Tokio 任务）
    {
        let worker_pool = pool.clone();
        let worker_storage = Arc::clone(&storage);
        tokio::spawn(async move {
            workers::transcode::run(worker_pool, worker_storage).await;
        });
    }

    // -----------------------------------------------------------------------
    // 路由定义
    // -----------------------------------------------------------------------

    let public_auth_routes = Router::new()
        .route("/login", post(login))
        .route("/register", post(register_user));

    let authenticated_user_routes = Router::new()
        .route("/", get(list_users))
        .route("/{id}", get(get_user).put(update_user))
        // 用户自行修改密码（需提供旧密码）
        .route("/{id}/change-password", put(change_password))
        .route_layer(auth_layer.clone());

    let admin_user_routes = Router::new()
        .route("/", post(create_user))
        .route("/{id}", delete(delete_user))
        // 管理员重置用户密码，重置后 password_reset_required = true
        .route("/{id}/reset-password", post(admin_reset_password))
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(auth_layer.clone())
        .route_layer(Extension(AllowedRoles::new([UserRole::Admin])));

    let teacher_major_read_routes = Router::new()
        .route("/", get(list_majors))
        .route("/{id}", get(get_major))
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(Extension(AllowedRoles::new([
            UserRole::Teacher,
            UserRole::Admin,
        ])))
        .route_layer(auth_layer.clone());

    let admin_major_write_routes = Router::new()
        .route("/", post(create_major))
        .route("/{id}", put(update_major).delete(delete_major))
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(Extension(AllowedRoles::new([UserRole::Admin])))
        .route_layer(auth_layer.clone());

    let major_routes = teacher_major_read_routes.merge(admin_major_write_routes);

    let user_routes = authenticated_user_routes.merge(admin_user_routes);

    // 课程管理列表（须先于 `/{id}` 注册，避免与 UUID 路由冲突）
    let course_manage_routes = Router::new()
        .route("/manage", get(list_courses_manage))
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(Extension(AllowedRoles::new([
            UserRole::Teacher,
            UserRole::Admin,
        ])))
        .route_layer(auth_layer.clone());

    // 课程公开路由：无需登录（门户列表仅已发布；详情/章节对未发布需带教师或管理员 Token）
    let public_course_routes = Router::new()
        .route("/", get(list_courses))
        .route(
            "/{course_id}/topics/highlight",
            get(list_topic_highlights),
        )
        .route("/{id}", get(get_course))
        .route("/{course_id}/chapters", get(list_chapters))
        .route("/{course_id}/chapters/{chapter_id}", get(get_chapter));

    // 课程写操作路由：仅 Teacher/Admin
    let teacher_course_routes = Router::new()
        .route("/{id}/cover/upload-url", post(request_course_cover_upload_url))
        .route("/{id}/cover/confirm", post(confirm_course_cover))
        .route("/{id}/cover", delete(delete_course_cover))
        .route("/", post(create_course))
        .route("/{id}", put(update_course).delete(delete_course))
        .route("/{course_id}/chapters", post(create_chapter))
        .route(
            "/{course_id}/chapters/{chapter_id}",
            put(update_chapter).delete(delete_chapter),
        )
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(Extension(AllowedRoles::new([
            UserRole::Teacher,
            UserRole::Admin,
        ])))
        .route_layer(auth_layer.clone());

    // 选课路由：任意已登录用户可选课 / 取消选课 / 查询状态 / 获取已选课程列表
    // /enrolled 须先于 /{course_id} 注册，避免被 UUID 路由拦截
    let enrollment_routes = Router::new()
        .route("/enrolled", get(list_enrolled_courses))
        // 选课人数统计必须先于 /{course_id}/enroll 注册，避免被参数路由拦截
        .route("/{course_id}/enroll/count", get(get_course_enrollment_count))
        .route(
            "/{course_id}/enroll",
            get(get_enrollment_status)
                .post(enroll_course)
                .delete(unenroll_course),
        )
        // 投票路由：角色和选课检查在 handler 内完成，路由层只负责认证
        .route("/{id}/vote", post(toggle_course_vote))
        .route_layer(auth_layer.clone());

    // 社区路由：话题、回复、@候选成员（均需登录）
    // 注意：/community/members 的字面量路径须先于 /{topic_id} 注册，避免被参数路由拦截
    let community_routes = Router::new()
        .route("/{course_id}/community/members", get(list_community_members))
        .route(
            "/{course_id}/topics",
            get(list_topics).post(create_topic),
        )
        .route(
            "/{course_id}/topics/{topic_id}",
            get(get_topic).delete(delete_topic),
        )
        .route(
            "/{course_id}/topics/{topic_id}/replies",
            get(list_replies).post(create_reply),
        )
        .route(
            "/{course_id}/topics/{topic_id}/replies/{reply_id}",
            delete(delete_reply),
        )
        .route_layer(auth_layer.clone());

    // 题库 + 试卷管理路由（写操作）：仅 Teacher/Admin
    let question_bank_routes = Router::new()
        // 题库导出/导入（字面量路径须在 /{question_id} 之前注册，Axum 按注册顺序匹配）
        .route("/{course_id}/questions/export", get(export_questions))
        .route("/{course_id}/questions/import", post(import_questions))
        // 题库 CRUD
        .route("/{course_id}/questions", get(list_questions).post(create_question))
        .route(
            "/{course_id}/questions/{question_id}",
            get(get_question).put(update_question).delete(delete_question),
        )
        // 试卷写操作（POST/PUT/DELETE）
        .route("/{course_id}/exams", post(create_exam))
        .route(
            "/{course_id}/exams/{exam_id}",
            put(update_exam).delete(delete_exam),
        )
        .route("/{course_id}/exams/{exam_id}/publish", post(toggle_exam_publish))
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(Extension(AllowedRoles::new([
            UserRole::Teacher,
            UserRole::Admin,
        ])))
        .route_layer(auth_layer.clone());

    // 试卷读取路由：已登录用户可访问（handler 内区分教师/学生权限）
    let exam_read_routes = Router::new()
        .route("/{course_id}/exams", get(list_exams))
        .route("/{course_id}/exams/{exam_id}", get(get_exam))
        .route_layer(auth_layer.clone());

    // 答题路由：已登录用户（handler 内校验选课状态 / 教师权限）
    // "my" 字面量路径必须在 /{attempt_id} 之前注册
    let attempt_routes = Router::new()
        .route(
            "/{course_id}/exams/{exam_id}/attempts/my",
            get(get_my_attempts),
        )
        .route(
            "/{course_id}/exams/{exam_id}/attempts",
            get(list_all_attempts).post(start_attempt),
        )
        .route(
            "/{course_id}/exams/{exam_id}/attempts/{attempt_id}",
            get(get_attempt_detail),
        )
        .route(
            "/{course_id}/exams/{exam_id}/attempts/{attempt_id}/submit",
            post(submit_attempt),
        )
        .route(
            "/{course_id}/exams/{exam_id}/attempts/{attempt_id}/grade",
            put(grade_attempt),
        )
        .route_layer(auth_layer.clone());

    let course_routes = Router::new()
        .merge(course_manage_routes)
        .merge(public_course_routes)
        .merge(teacher_course_routes)
        .merge(enrollment_routes)
        .merge(community_routes)
        .merge(question_bank_routes)
        .merge(exam_read_routes)
        .merge(attempt_routes);

    // 独立社区话题路由（不依附课程，挂载在 /api/community 下）
    // 话题列表和详情：任意已登录用户可访问
    let standalone_community_read_routes = Router::new()
        .route("/topics", get(list_standalone_topics))
        .route("/topics/{topic_id}", get(get_standalone_topic))
        .route_layer(auth_layer.clone());

    // 开设话题：仅教师 / 管理员
    let standalone_community_create_routes = Router::new()
        .route("/topics", post(create_standalone_topic))
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(Extension(AllowedRoles::new([
            UserRole::Teacher,
            UserRole::Admin,
        ])))
        .route_layer(auth_layer.clone());

    // 删除话题：任意已登录用户（handler 内校验作者/管理员权限）
    let standalone_community_delete_topic_routes = Router::new()
        .route("/topics/{topic_id}", delete(delete_standalone_topic))
        .route_layer(auth_layer.clone());

    // 加入/退出/成员列表/回复：任意已登录用户（handler 内校验成员权限）
    let standalone_community_member_routes = Router::new()
        .route(
            "/topics/{topic_id}/join",
            post(join_standalone_topic).delete(leave_standalone_topic),
        )
        .route("/topics/{topic_id}/members", get(list_standalone_topic_members))
        .route(
            "/topics/{topic_id}/replies",
            get(list_standalone_topic_replies).post(create_standalone_topic_reply),
        )
        .route(
            "/topics/{topic_id}/replies/{reply_id}",
            delete(delete_standalone_topic_reply),
        )
        .route_layer(auth_layer.clone());

    let standalone_community_routes = Router::new()
        .merge(standalone_community_read_routes)
        .merge(standalone_community_create_routes)
        .merge(standalone_community_delete_topic_routes)
        .merge(standalone_community_member_routes);

    // 章节下的视频列表（无需登录）
    let public_chapter_video_routes = Router::new().route("/{chapter_id}/videos", get(list_videos));

    // 视频读取路由（任意已登录用户）
    let authenticated_video_routes = Router::new()
        .route("/{id}", get(get_video))
        .route("/{id}/transcodes", get(get_transcodes))
        // 签发 HLS 播放 URL（需要登录）
        .route("/{id}/hls-url", post(create_hls_url))
        .route_layer(auth_layer.clone());

    // 视频写操作路由（仅 Teacher/Admin）
    let teacher_video_routes = Router::new()
        // 在章节下创建视频
        .route("/{id}/upload-url", post(request_upload_url))
        .route("/{id}/confirm-upload", post(confirm_upload))
        .route("/{id}", put(update_video).delete(delete_video))
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(Extension(AllowedRoles::new([
            UserRole::Teacher,
            UserRole::Admin,
        ])))
        .route_layer(auth_layer.clone());

    // HLS 播放路由：无需登录，通过 URL 中的 hls_token 进行鉴权
    // 静态字面量 playlist.m3u8 必须在通配符 {segment} 之前注册
    let hls_video_routes = Router::new()
        .route("/{id}/hls/{resolution}/playlist.m3u8", get(hls_playlist))
        .route("/{id}/hls/{resolution}/{segment}", get(hls_segment));

    // 在章节路由下挂载"创建视频"（POST /api/chapters/:chapter_id/videos）
    let teacher_chapter_video_routes = Router::new()
        .route("/{chapter_id}/videos", post(create_video))
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(Extension(AllowedRoles::new([
            UserRole::Teacher,
            UserRole::Admin,
        ])))
        .route_layer(auth_layer.clone());

    let video_routes = authenticated_video_routes
        .merge(teacher_video_routes)
        .merge(hls_video_routes);

    // 发现页接口：热门课程 / 活跃教师 / 最新话题（均无需登录）
    let discover_routes = Router::new()
        .route("/popular-courses", get(popular_courses))
        .route("/active-teachers", get(active_teachers))
        .route("/latest-topics", get(latest_topics));

    // 试卷组题路由（通过 /api/exams 前缀访问，与课程解耦）：仅 Teacher/Admin
    let exam_composition_routes = Router::new()
        .route("/{exam_id}/questions", get(list_exam_questions).post(add_question_to_exam))
        .route(
            "/{exam_id}/questions/{entry_id}",
            put(update_question_in_exam).delete(remove_question_from_exam),
        )
        .route_layer(from_fn(require_roles_middleware))
        .route_layer(Extension(AllowedRoles::new([
            UserRole::Teacher,
            UserRole::Admin,
        ])))
        .route_layer(auth_layer.clone());

    let app = Router::new()
        .route("/", get(|| async { "QXNZY 微课平台后端服务运行中" }))
        .nest("/api/auth", public_auth_routes)
        .nest("/api/users", user_routes)
        .nest("/api/majors", major_routes)
        .nest("/api/courses", course_routes)
        .nest("/api/community", standalone_community_routes)
        .nest("/api/discover", discover_routes)
        // 视频相关路由
        .nest("/api/videos", video_routes)
        .nest(
            "/api/chapters",
            public_chapter_video_routes.merge(teacher_chapter_video_routes),
        )
        // 题库 + 试卷路由
        .nest("/api/exams", exam_composition_routes)
        // State 必须在 Extension 之前完成；Extension 放在 with_state 之后，否则列表等 handler 取不到 MinIO 客户端（会 500）
        .with_state(pool)
        .layer(Extension(storage));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("服务器已启动：http://localhost:8080");
    axum::serve(listener, app).await.unwrap();
}
