# REST 测试说明

本目录为 [REST Client](https://marketplace.visualstudio.com/items?itemName=humao.rest-client)（VS Code / Cursor）可用的 `.http` 文件。

## 服务地址

- **Base URL**：`http://localhost:8080`（与 [`src/main.rs`](../src/main.rs) 中 `TcpListener::bind("0.0.0.0:8080")` 一致）
- 若本地改端口，请同步修改各文件顶部的 `@base` / `@base_url` 变量

## 文件索引

| 文件 | 说明 |
|------|------|
| [auth.http](./auth.http) | `POST /api/auth/login`、`POST /api/auth/register` |
| [users.http](./users.http) | `GET/POST/PUT/DELETE /api/users` |
| [majors.http](./majors.http) | `GET/POST/PUT/DELETE /api/majors`（读需教师或管理员 Token；写需管理员） |
| [courses.http](./courses.http) | 课程列表/详情/管理、封面直传 |
| [enrollments.http](./enrollments.http) | `GET/POST/DELETE /api/courses/:course_id/enroll`（选课/取消选课/查询状态） |
| [chapters.http](./chapters.http) | `GET/POST/PUT/DELETE /api/courses/:course_id/chapters` |
| [videos.http](./videos.http) | 章节下创建视频、上传、转码、HLS 播放 |

## 认证

多数接口需请求头：

```http
Authorization: Bearer <JWT>
```

登录成功后响应中的 `token` 即 JWT，除 `GET /api/courses`（门户列表）等公开接口外，按各文件说明替换占位符。

## 各文件内的文档

每个 `.http` 文件顶部与分节处包含：

- **路径与方法**（与路由注册一致）
- **请求体 JSON 字段说明**（`Content-Type: application/json`）
- **响应示例**（`200`/`201` 等成功时的 JSON 结构示意；实际 UUID、时间戳以服务端为准）
