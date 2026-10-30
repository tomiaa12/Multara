# Rust 后端卡顿与内存审计：主线程阻塞、锁争用、内存增长

范围：`src-tauri/src` 全部（命令、数据库、运行管线、网关代理、终端、LSP、文件监听、搜索、Git、MCP、渠道、日志）。已在前两份文档里处理的内容（网关碎片、逐 token 落库已批处理、`list_chat_messages` 异步化、`emit_deleted_snapshots` 逐行拿锁、`is_stopped` 节流、连续工具报错熔断）这里不再重复。

方法：按三个区域并行只读排查，我再回到代码里核对影响最大的结论。标记含义：

- **[已核对]**：我在代码里亲自看过。
- **[排查所得]**：排查时读代码得出，我没有再逐条复核。
- **[推断]**：按机制推断，没有验证。

**这份文档没有改任何代码，也没有做性能录制。** 所有“慢”和“内存涨”是按代码和触发条件判断的。唯一有实测数据的是 `multara.log` 里的耗时记录，文中会注明。

---

## 0. 先读这个：最可能解释“用起来卡”的 6 件事

| # | 现象 | 根因 | 位置 | 级别 |
|---|---|---|---|---|
| 1 | 每次启动窗口发白、冻几秒；打开运行时设置、保存智能体时界面卡住 | `runtime::detect()` 每次都起一个**交互式登录 shell**（`$SHELL -ilc`，最长 10 秒，最多 3 次回退）并逐个跑 `--version`（每个最长 8 秒）；它被 `seed()` 在启动时无条件调用，也被一批同步命令在**主线程且持有数据库锁**时调用 | `runtime.rs:198`、`database.rs:929`、`commands.rs:1294` 等 | 高 |
| 2 | `pnpm tauri:dev` 下一切 Rust 侧都慢 | `Cargo.toml` 里**没有任何 `[profile]`**，开发构建连依赖（SQLite、regex、serde_json、base64…）都是 opt-level 0 | `Cargo.toml` | 高 |
| 3 | 智能体每调用一次 `multara` 命令行就有明显延迟，还和应用抢锁 | CLI 每次调用都 `open_file`：完整 migrate + seed（里面又是 `detect()` 和登录 shell） | `cli.rs:478` | 高 |
| 4 | 历史越多越卡：收件箱、任务列表、会话切换变慢 | `agent_runs`、`messages` 几乎没有索引，热查询是全表扫描 + 相关子查询 | `database.rs`、`ops/inbox.rs:7`、`ops/task.rs:400` | 高 |
| 5 | 带通配符的搜索在大仓库里特别慢 | `glob_match` 对**每个文件、每个模式**都 `Regex::new` 一次 | `text_search.rs:243` | 高 |
| 6 | 终端卡住时整个窗口一起冻 | `terminal_write` 在全局终端锁内做阻塞写；它是同步命令，跑在主线程 | `terminal.rs:187`、`commands.rs:173` | 中~高 |

**有实测佐证的**：`multara.log` 里 `bootstrap` 出现过 2.3 秒和 6.4 秒的 SLOW 记录，`list_runtime_models` 反复出现 2.5~3.5 秒，这和第 1 条（`detect()` 起登录 shell、跑版本探测）的机制吻合，但我没有用日志直接证明是它。

---

## 1. 主线程阻塞

Tauri 2 里，非 `async` 的 `#[tauri::command]` 跑在主线程。全部命令里同步的有 111 个（`commands.rs` 102、`explorer_fs.rs` 7、`lsp.rs` 2）；`merge_flow.rs` 和 `native_pick.rs` 都是异步。其中约 55 个只做很轻的数据库或本地操作，但**几乎都要先拿全局数据库锁**，所以只要有人长时间持锁，这些轻命令也会被一起卡住。

### 1.1 持有数据库锁时做慢操作（高）

| 命令 | 位置 | 做什么 | 来源 |
|---|---|---|---|
| `detect_runtimes`、`configure_runtime`、`save_custom_runtime`、`delete_custom_runtime` | `commands.rs:1294`、`1350`、`1516`、`1638`；`runtimes(&db)` 在 `6196` | 持锁调用 `runtime::detect()`（登录 shell + 版本探测） | 排查所得 |
| `save_agent` | `commands.rs:2849`，经 `ops/agent.rs:137` | 持锁调用 `detect()` | 排查所得 |
| `detach_workspace`、`delete_project` | `commands.rs:1028`、`1097` | `git worktree remove` 和 `remove_dir_all` 持锁执行 | 排查所得 |
| `scan_resources`、`list_resource_sources`、`list_prompt_files`、`save_mcp`、`install_mcp`、`delete_mcp`、`sign_out_mcp`、`set_mcp_enabled`、`delete_skill`、`set_skill_enabled` 等 | `commands.rs:1947`、`1954`、`2071`、`2621`… | `scan_all`（`1941`）遍历所有 CLI 与工作区的技能 / MCP 目录，持锁，每次返回完整资源列表 | `scan_resources` 已核对 |
| `truncate_session_from_message` | `commands.rs:4173` | 锁一直持到函数结束，其中包含 `remove_dir_all` | 排查所得 |
| `load_active_diff_data`、`load_session_diff_data` | `commands.rs:4075`、`4051` | 持锁时对**所有**快照和基线做 zstd 解压并整体返回，见 §3.1 | 已核对 |
| `save_settings` | `commands.rs:2548` | 语言变化时 `builtin_skills::materialize` 写文件，持锁 | 排查所得 |

### 1.2 在主线程做网络 / 大模型 / 子进程（高）

| 命令 | 位置 | 阻塞多久 |
|---|---|---|
| `generate_chat_session_title` | `commands.rs:3361` | 调一次 CLI / LLM，超时 8 秒，**在主线程** |
| `compact_chat_session` | `commands.rs:4742` | 调 LLM，超时 90 秒，**在主线程** |
| `explorer_clipboard_read_files` / `write_files` | `explorer_fs.rs:763`、`749` | macOS 上同步起 `osascript` |
| `scan_workspace_candidates` | `commands.rs:1282` | 对每个子目录起 git 子进程（`find_git_root`、`for-each-ref`） |
| `check_git_executable`、`probe_runtime_command` | `commands.rs:2608`、`1668` | 起 `git --version`；`which` 加可能的登录 shell 加载 |
| `stop_run` | `commands.rs:4421` | `executor::stop`：等锁、起 `kill` 子进程、走完整个 `finish` 事务（含快照清理） |

### 1.3 在主线程读写大文件（高）

`read_workspace_file`（`commands.rs:232`）和 `read_external_file`（`243`）同步 `fs::read` 整个文件，上限 128MB（`files.rs:11-13`），媒体最多 32MB 再 base64，外加一次数据库锁；`write_workspace_file`（`255`）同步写整文件；`delete_workspace_entry`（`302`）里是递归 `remove_dir_all`（比如 `node_modules`）；`load_attachment_preview`（`3623`）最多读 10MB 再 base64；`save_pasted_attachment`（`3714`）最多 25MB 的 `Vec<u8>` 走 IPC。**[已核对]**（`read_workspace_file`、`scan_resources`、`load_active_diff_data` 的签名都是同步 `pub fn`）

当前版本的日志里 `read_workspace_file` 150 次累计约 0.1 秒，所以常规文件没问题，风险在**大文件**和**数据库锁被占用**的时候。

### 1.4 终端、LSP、监听器

- **终端写入锁内阻塞（中~高）[已核对]**：`terminal.rs:187-196` 的 `write()` 在持有全局 `SESSIONS` 锁时 `write_all` + `flush`。如果 PTY 输入缓冲满（粘贴大段内容而子进程没在读、或作业被挂起），`write_all` 会一直阻塞，而 `resize`、`close`、`open` 和每个终端的退出监视线程都要同一把锁；`terminal_write` / `terminal_resize` / `terminal_close` 又是主线程命令，**整个 UI 会一起冻住**。
- **`terminal_publish`（中）[排查所得]**：同步命令，每次都在 `snapshot_io` 锁里整文件写终端文本，再读、解析、重写 `index.json`。前端传多大就写多大。
- **`explorer_watch_workspaces`（中）[排查所得]**：同步命令，在主线程对每个工作区建立递归 `notify` 监听（`explorer_fs.rs:1148`）。`excludes` 只在回调里生效，系统事件照样全部到达；改排除规则会把所有工作区的监听器全部拆掉重建，也在主线程。
- **LSP 并发启动竞态（中）[排查所得]**：`lsp.rs:104-140` 里“是否已在运行”的检查释放锁之后才 spawn 和 insert；两个并发的 `lsp_start`（切工作区、组件重新挂载）会各起一个 rust-analyzer，那是动辄数 GB 的进程。

---

## 2. 内存增长与泄漏

### 2.1 单次运行的缓冲没有上限（高）[排查所得 + 部分已核对]

- `output: Arc<Mutex<String>>`（`executor.rs:1200`）累积全部可见文本，没有上限；`previous`、`PendingStream.item/chunk` 各再持有一份当前合并后的文本。合并结果没有上限（`truncate_and_redact` 只限制每个**进来的**块，64k 字符），峰值约总文本长度的 3~4 倍，结束时还有一次 `output.lock().clone()`（`:1316`）。
- 没有任何“每次运行最多多少事件 / 字节”的限制；`run_timeout` 默认 0（无限，`executor.rs:1394-1413`）；唯一的刹车是我加的连续 25 次 `<tool_use_error>`。
- **读行无上限 [已核对]**：`executor.rs:1455` 是 `BufReader::new(stream).lines()`。一个 CLI 打印一行 500MB，就会分配整行（缓冲增长时峰值约 1GB），再被 JSON 解析多次（`id_from_line`、`usage.ingest`、`is_stream_turn_result`、`normalize_runtime_line`）；非 JSON 行进 `truncate_and_redact`，会把整行字符串复制三次才截断，峰值可达数 GB。
- **合并仍是逐 token 平方级内存拷贝 [排查所得]**：`merge_timeline_item`（`executor.rs:3450-3482`）每个 token 都 `format!(前+后)`、`clone()`、再对整段累计文本 `serde_json::from_str`，再加 `previous = Some(item.clone())`，每个 token 至少 3 份全文拷贝。我上一轮把**落库和发事件**批处理了，这里的**内存拷贝**没有动。200KB 回答约 5 万 token，量级是数十 GB 的 memcpy（排查者给的估算：约 10GB）。
- **每次运行 5~8 个线程 [排查所得]**：supervisor + stdout 读线程 + flusher + stderr 读线程 + flusher，再加 stdin 写线程、协议驱动线程、网关代理线程。supervisor 还每 120ms 查一次 `is_stopped`（一次数据库查询，`executor.rs:1282-1303`）。

### 2.2 网关代理（中）[已核对 + 排查所得]

- **单线程串行处理请求 [已核对]**：`gateway_proxy/mod.rs:132-145` 用 `server.recv_timeout` 然后串行 `handle_request`，`respond_reader` 一直阻塞到整条 SSE 流结束（最长 600 秒，`curl --max-time`）。一个长流式请求会让其它请求（并行子智能体、标题生成）全部排队；客户端不读数据时没有写超时，会无限期卡住。每次运行起一个代理。
- 每个上游请求 fork 一个 `curl` 子进程，配 2~3 个临时文件，没有连接复用（排查所得）。
- 非流式响应全量缓冲且没有上限（`mod.rs:274` 的 `read_to_string`）；`http_client::request` 先把整个响应写文件再 `fs::read`，之后才检查 8MB 上限（`http_client.rs:98-103`）；请求体上限 32MB（`mod.rs:211`），会产生约 5 份拷贝。
- `request_stream` 早期错误时丢弃 `child` 不 kill 不 wait，会留下一个 curl 直到 600 秒超时，并成为僵尸进程；`GatewayProxy::drop` 会 join 服务线程，运行结束后如果还有在途请求，supervisor 线程会等最多 600 秒。
- `read_event` 的 `read_line` 无上限（`streaming.rs:398-429`）。

### 2.3 进程与线程泄漏

- **终端关闭后留僵尸进程 [排查所得，推断近乎确定]**：`Session::drop` 只 `child.kill()` 不 `wait()`（`terminal.rs:23-27`，已核对），`close()` 之后退出监视线程也不收尸，每个手动关闭的终端会留下 defunct 进程直到应用退出；只杀了 shell 本身，`nohup` 的后台作业和不同进程组的进程会存活。
- **`lsp_stop` 留僵尸 [排查所得]**：移除后才 `kill`，`read_loop` 之后 `remove` 得到 `None`，不会走到 `wait`；也没有在关闭工作区或退出应用时停掉 LSP。
- **子进程管道可让运行永远“进行中” [推断]**：子进程退出后 `stdout_reader.join()` 等 EOF；如果有孙进程继承了 stdout（后台起的开发服务器），EOF 永远不来，`finish` 不被调用，运行一直占着并发槽，而默认没有超时。同类：stdout 里出现非法 UTF-8 时 `lines()` 返回 Err，`map_while(Result::ok)` 静默结束，管道不再被读，64KB 满后子进程阻塞，运行挂住直到手动停止。
- **权限等待拖慢收尾 [推断]**：`PermissionGate::decide` 最长 `recv_timeout(300s)`，停止后 `finish` 的最终保存最多被延迟 5 分钟。
- **MCP 探测 [推断]**：只杀直接子进程，`npx` / `uvx` / `sh -c` 包装的真实服务器孙进程会成为孤儿；`mcp_tools_cache` 串行探测，每个最长 40 秒。
- **渠道（飞书）[排查所得]**：`chunks` 映射在消息分片永远收不全时不会清理，`vec![None; sum]` 的 `sum` 来自服务端头部没有上限，每个入站事件起一个线程且无上限。

### 2.4 磁盘与表的无限增长（中）

- **`run_events`、`messages`、`agent_runs`**：没有保留策略，只有删任务 / 会话 / 项目或截断时才删（排查所得，我之前在本机库里看到 `run_events` 已有约 3.7 万行，其中 `output` 和 `input_json` 是最大的体积）。
- **`automation_runs`、`webhook_deliveries`、`channel_run_deliveries`、`channel_outbound_messages`**：无清理（只有 `channel_inbound_dedup` 会清）。
- **`turn_file_snapshots` / `diff_chunk_states`**：限制在最近 30 个会话和 500MB（压缩后），但**只在启动的 migrate 里清理**，长时间运行不会清；`file_review_baselines` 只在 detach 时删，没有清理；没有任何 `VACUUM`、`ANALYZE`、`PRAGMA optimize`，文件不会缩小。
- **日志**：`diag.rs` 只在 `init()` 里按 8MiB 轮转一次（`diag.rs:24`、`159`，已核对），长时间运行不轮转，这正是 `multara.log.old` 有 37MB 的原因；每行都 `flush()`，116 处 `diag::span` 每次写两行，加每 15 秒心跳。
- **粘贴的图片**：`temp_dir()/multara-paste` 里最多 25MB 一张，**从不删除**。
- **终端文本快照**：只有 `terminal_close` 才删，崩溃会留孤儿文件，`index.json` 里留着失效条目。
- `mcp-tools-cache.json` 条目不会被淘汰（推断）。

---

## 3. 数据库

### 3.1 一把全局互斥锁 + 默认的 FULL 同步（高）

- 所有数据库访问走一个 `Mutex<Connection>`；`diag::lock_mutex` 的行为 **[已核对]**：`try_lock` 失败后 `sleep(25ms)` 再试，**不烧 CPU**，但每次争用最多多 25ms 延迟，且不公平，流式写入者可能让主线程的等待者一直拿不到锁；等超过 2 秒才写 WAIT 日志。
- PRAGMA 只设了 `busy_timeout=5s`、`WAL`、`foreign_keys=ON`（`database.rs:55-60`，已核对）。**没有 `synchronous=NORMAL`**，默认的 FULL 意味着每次提交一次 fsync；没有 `cache_size`（默认约 2MB）、`mmap_size`、`temp_store`、`journal_size_limit`。
- 我上一轮把正文的落库批成约每 80ms 一次，已经大幅降低了 fsync 次数，但 FULL 的代价仍在每次提交上。

### 3.2 缺索引导致热查询全表扫描（高）[已核对索引清单；各查询的扫描结论为排查所得]

现有索引只有 `run_events(run_id, seq)`、`run_events(session_id, run_id, seq)`（我上一轮加的）、`message_attachments(message_id)`、`workspaces(project_id)`、`session_projects`、`agent_turns(session_id, seq)`、快照相关、`automation_runs`，以及 `tasks.session_id`、`inbox_items.session_id` 的唯一约束。**`messages` 和 `agent_runs` 完全没有索引**（我核对过：`CREATE INDEX` 里没有这两张表）。

| 查询 | 位置 | 问题 |
|---|---|---|
| 收件箱列表 | `ops/inbox.rs:7-26` | 每个未读行一个相关子查询 `COUNT(*) FROM messages WHERE session_id=…`，外加每条消息的锚点子查询，复杂度约 未读数 × 全部消息；`list_inbox`（bootstrap、`list_task_inbox_surfaces`）和 `mark_inbox_all_read`（`inbox.rs:79`）都走这里 |
| 任务列表 | `ops/task.rs:400-435`（`settle_finished`） | 每次 `list_tasks`（bootstrap、`delete_task`）对每个进行中任务做 3 个相关子查询扫 `agent_runs`，带 `ORDER BY COALESCE(...) LIMIT 1` |
| 调度器每 2 秒 | `executor.rs:791-857`（`dispatch_pending`） | `status='queued'` 加两个相关 `COUNT(*) … status='running'`，`agent_runs.status` 无索引，**每 2 秒一次全表扫描并持锁**，随运行历史线性变慢 |
| 活动快照加载 | `snapshot_db.rs:310-340` | 取全部候选行的 `content` BLOB；`id = (SELECT … LIMIT 1)` 相关自连接；每行 `EXISTS(messages)` 和对分组 CTE 的 `NOT EXISTS`；之后全部解压成字符串整体走 IPC；`load_review_baselines`（`:228`）没有 WHERE，也是全量 |
| 会话列表 | `commands.rs:4784` | 无 LIMIT，且 N+1（`session::linked_project_ids` 每个会话一次 EXISTS 加 prepare/query） |
| 统计 | `commands.rs:4834`、`5289+` | `stats()` 对 `agent_runs` 扫 4 遍；`run_statistics` 扫 4~5 遍，`date(created_at,'localtime')` 使索引失效，还可能每个 run 一个 EXISTS |
| 任务全量 | `ops/task.rs:437` | `list` 无 LIMIT，每次 `update_task_status` / `delete_task` 都整体返回 |
| 截断会话 | `commands.rs:4253+` | 每条消息 4 个 DELETE、每个 run 2 个；`agent_turns.user_message_id`、`agent_runs.turn_id` 无索引，每个都是一次全表扫描（N+1 × 扫描） |
| 按智能体查运行 | `commands.rs:3069` | `agent_runs WHERE agent_id=?` 无索引，全扫加排序 |

建议新增：`messages(session_id, created_at)`、`messages(run_id)`、`agent_runs(session_id)`、`agent_runs(status)`、`agent_runs(agent_id, created_at)`、`agent_runs(turn_id)`、`agent_turns(user_message_id)`、`tasks(status, updated_at)`、`inbox_items(archived_at, created_at)`、`chat_sessions(updated_at, project_id)`。

### 3.3 `capture_file_before_edit` 先读整个文件再检查大小（高）[已核对]

`executor.rs:2134-2144` 持锁，然后 `snapshot.rs:122` `fs::read(&abs_path)` 读**整个文件**，到 `:127` 才判断 `len() > MAX_FILE_BYTES`（2MB）。一个多 GB 的文件会在持有全局锁时被完整读进内存。2MB 以内的文件还要在锁内做 sha256 + zstd 压缩 + 第二次压缩（`ensure_review_baseline`），并把最多 2MB 的 `snapshotContent` 通过 IPC 发出去。应先看 `metadata().len()`。

### 3.4 逐行写库（中~高）[已核对]

- `conversation.rs:248-262` 的 `persist_from_line` 对**每一行** stdout 解析 JSON、找 `session_id`、拿 `lock_db()` 并执行 `INSERT … ON CONFLICT DO UPDATE`，没有变化检测。Claude stream-json 几乎每行都带 `session_id`（包括增量片段），所以这是和时间线落库**分开的**每 token 一次写加一次抢锁；同一行还被解析 3~4 次。`persist_run_usage`（`executor.rs:1468`）对带用量的每一行再做一次 UPDATE。
- **stderr 每行一个事件 [已核对]**：`executor.rs:2636-2642`，`is_error` 时 `normalize_runtime_line` 对每个非空 stderr 行返回一个 `error` 事件，不合并、不批处理，每个都 `persist_timeline_item`（拿全局锁）并 `app.emit`，新 seq。噪声大的 CLI（进度条、警告、调试日志）会产生无限多 `run_events` 行和等量 IPC 事件。另有一个小 bug：`stderr_output` 从不被追加（`visible_text` 只对 `text` 事件设置），所以 `:1336` 的失败详情永远是空的。

### 3.5 持有数据库锁时做 IO 的完整清单

1. `ops/agent.rs:137`（经 `commands.rs:2849`）：`runtime::detect()`，主线程。
2. `commands.rs:1294-1297`、`1364-1366`、`1585`、`1664`：`runtimes(&db)` → `detect()`。
3. `executor.rs:2134-2144`：整文件读 + zstd + sha256。
4. `commands.rs:4075-4081`、`4056`：解压全部快照与基线。
5. `commands.rs:1031-1050`（git worktree）、`1097-1105`（`workspace::remove` 循环）、`1271`（`record_origin`，2 个 git 子进程）。
6. `commands.rs:1941-1944` 的 `scan_all` 及其所有调用者（`1950`、`2051`、`2065`、`2640`、`2667`、`2690`、`2724`、`2737`）。
7. `commands.rs:2594-2603`（`save_settings`）、`4230`（`truncate_session_from_message` 的 `remove_dir_all`）。
8. `executor.rs:1664-1832`（`finish`）：整个序列持锁，包括 `dispatch_squad_members`、`sync_from_run`、`maybe_resume_parent`、`save_terminal_message` 和 `cleanup_old_snapshots`（全表 GROUP BY + SUM）；没有子进程或网络 IO，但每个运行结束都占一次长锁。
9. `executor.rs:3495`：`persist_timeline_item` 每次 flush 拿一次锁（受 FULL 同步影响）。

确认没问题（锁在 IO 之前已释放）：`resource_context`、`workspace_context`、`generate_text`、`merge_flow.rs` 全部 git 工作、`emit_deleted_snapshots`（已批处理）。

---

## 4. 启动路径

`lib.rs:92-127`：窗口先 `reveal_main_window`，然后 `setup` 继续**阻塞主线程**，用户看到的是白屏或冻结的窗口。`setup` 里依次是：

1. `database::open` → `migrate`：57 次 `ensure_column`（每次一个 `PRAGMA table_info`）[已核对数量]、表重建检查、`backfill_file_review_baselines`、`cleanup_old_snapshots`（对快照 `SUM(LENGTH(content))`）、`automation::backfill_results`。每次启动都跑；在 30MB 的库上都便宜，快照那个求和在数据多时会慢；基线为空时的回填会解压所有活动快照。
2. **`seed()`（`database.rs:929`）无条件调用 `runtime::seed_runtime_id()` → `detect()`** [已核对]，即使智能体已存在。`detect()` 每次都先 `refresh_login_path_dirs()`（`runtime.rs:198-203`，已核对），起 `$SHELL -ilc` 并最多 3 次回退，再对每个运行时规格顺序跑 `--version`（每个最长 8 秒）。缓存只在进程内，每次启动都是冷的。`.zshrc` 里有 nvm 之类就会多出数秒。
3. `recover_interrupted_runs`：廉价，但对 `agent_runs` 做一次 `status='running'` 全扫。
4. `webhook::start` 同步拿一次锁；`executor::start_poller`（2 秒）、`channel::supervisor::start`（3 秒）、`automation::start`（30 秒）在线程里。
5. 之后前端调用的 `bootstrap` 是异步的，但里面又调用一次 `detect()`（再起一次登录 shell）和 `resource_manager::scan()`。

**CLI 路径同样的开销 [已核对]**：`cli.rs:478` 每次调用都 `database::open_file`，即完整 migrate + seed（含 `detect()`、登录 shell、`--version` 探测），而智能体每用一次工具就可能调一次 `multara`。每次还要拿写锁，和应用自己的连接争用（`busy_timeout` 5 秒）。

---

## 5. 搜索、Git、文件监听、其它

### 5.1 搜索（高~中）

- **每个文件编译一次正则 [已核对]**：`text_search.rs:243` `Regex::new(&format!("^{}$", glob_source(...)))` 在 `glob_match` 里，`file_included` 对每个文件、每个包含 / 排除模式各调一次。开发构建是 opt-level 0 时更严重。
- 目录遍历是手写的（`text_search.rs:75-135`）：跟随符号链接目录，没有深度上限和已访问集合，链接环会递归到 ELOOP；**不遵守 `.gitignore`**，只有静态的 `SEARCH_EXCLUDE_DIRS`；每个候选文件（最多 1MiB）整个 `fs::read` 再 `from_utf8_lossy` 复制一次；单线程；**不可取消**（`commands.rs:91` 每次按键起一个不可取消的 `spawn_blocking`）；匹配数上限 2000，但**零匹配的查询要走完整棵树**。（均为排查所得；前端不防抖已在上一份文档里核对。）

### 5.2 Git（中）[排查所得]

- `git.rs:28-60` 的 `status()` 每次调用串行起 **8 个 git 进程**（`status --porcelain`、`branch --show-current`、`rev-parse @{upstream}`、`rev-list --left-right --count`、`remote`、两次 `for-each-ref`、`rev-parse --git-path MERGE_HEAD`），没有超时；`status --porcelain` 默认列出未跟踪文件，且整个 `src` 里没有 `--no-optional-locks`，后台轮询会拿 `index.lock`，可能和用户自己的 git 命令撞上（“index.lock exists”）。
- `output()` 把整个 stdout 读进内存：`show_spec`（`git.rs:925`，`revision_file` / `index_file` 用）会把整个 blob 读出来再通过 IPC 返回，只用 `\0` 判断二进制，500MB 文件就是 500MB 字符串；`staged_diff`（`:397`）先加载整个 `git diff --cached` 再截到 24k 字符；`blame_file` 加载整个 porcelain 输出（这是我上一轮加的命令，同样没有大小保护）。
- `revision_file` / `index_file` / `blame_*` 每次先起一次 `rev-parse --is-inside-work-tree`，所以每次调用是 2 个进程。
- `run_with_timeout` 杀进程后 join 读线程，如果钩子留下的孙进程握着管道，`join()` 会超过超时；`explorer_git_ignored`（`explorer_fs.rs:633`）先写完全部 stdin 再读 stdout，路径列表很大时可能死锁；`resources.rs:944-975` 的 `git clone --depth 1` 没有超时，也没有 `GIT_TERMINAL_PROMPT=0`，凭据提示会永远挂住。

### 5.3 文件监听（中）[排查所得]

`explorer_fs.rs:1059-1160`：`watcher.watch(&root_path, RecursiveMode::Recursive)` 监听整棵树，`excludes` 只在回调里生效。macOS 的 FSEvents 注册便宜，但 `node_modules`、`target`、`.git/objects` 里的每个事件仍会进到回调，每个都跑 `relative_change` 和 N 个正则；Linux 的 inotify 会为每个子目录加 watch，可能触到 `max_user_watches`。已有的缓解（30k 事件的有界通道、每批 500 路径、200ms 节流、75ms 防抖）是好的。`list_directory` 对每个条目一次 `fs::metadata` 且无上限，展开 `node_modules` 根目录会 stat 上千个条目。

### 5.4 终端子进程轮询（中）[排查所得]

`terminal.rs:560-640` 的 `services()` 每次整张进程表 `ps -ax` 加整机 `lsof -nP -iTCP -sTCP:LISTEN`（macOS 上 `lsof` 常需 0.3~3 秒 CPU），都用 `.output()` 无超时、无去重，重叠的轮询会堆积 `spawn_blocking` 任务；前端每 2 秒轮询一次（上一份文档已提）。

### 5.5 日志与同步（低）

- `diag.rs` 每行一次锁、一次 `chrono::Local::now()` 格式化、`writeln!` 加显式 `flush()`；线程 id 每行 `format!` 再 `replace`。
- `channel/supervisor.rs:23-29` 每 3 秒拿一次数据库锁并解析所有安装配置，即使一个渠道都没配；`backoff` 只增不重置。
- `code_index.rs` 有界（3000 文件 / 每个 512KiB / 总 16MiB；类型声明 4000 文件 / 24MiB），用了 `ignore::WalkBuilder` 并遵守 `.gitignore`；但 `collect_typings` 没有 mtime 增量，每次调用重新发最多 24MiB；大小上限用 `continue` 而不是 `break`，文件数有界但时间无界。
- `lsp.rs:190-210` `read_message` 用 `vec![0; length]`，`length` 来自服务端的 `Content-Length`，没有上限，`read_line` 读头部也无上限；每条消息再 `from_utf8_lossy` 复制一次，无批处理地 `app.emit`。

### 5.6 `Cargo.toml`（高，开发体验）[已核对]

没有任何 `[profile.*]` 段，上层也没有工作区 `Cargo.toml` 或 `.cargo/config.toml`。`pnpm tauri:dev` 下一切都是 opt-level 0，包括 SQLite（rusqlite bundled）、`regex`、`ignore`、`base64`、`notify`、`zstd`、`serde_json`。这让 SQLite、正则、git 输出解析、PTY 数据块的 base64、IPC 的 JSON 序列化都比发布版慢数倍。Release 也没有 lto，`codegen-units=16`，`panic=unwind`。

---

## 6. 建议的处理顺序

**第一批：几乎不冒风险，收益最大**
1. `Cargo.toml` 加 `[profile.dev.package."*"] opt-level = 3`（或 2）和 `[profile.dev] opt-level = 1`；release 加 `lto = "thin"`、`codegen-units = 1`。
2. 缓存 `runtime::detect()`：不要每次刷新登录 PATH（只在用户点“重新检测”时刷新），启动的 `seed()` 里不要调它，所有调用点移出数据库锁和主线程。
3. 加 §3.2 的索引；重写收件箱未读计数查询；`dispatch_pending` 加 `agent_runs(status)`。
4. `database.rs` 加 `PRAGMA synchronous=NORMAL`、`cache_size`、`journal_size_limit`，再定期 `PRAGMA optimize`。
5. `glob_match` 里把正则预编译成一次（搜索开始时编译全部包含 / 排除模式）。

**第二批：把重命令移出主线程**
6. 把 §1.1~1.3 里的命令改成 `async` + `spawn_blocking`：`detect_runtimes` 等一组、`scan_*` 一组、`read/write_workspace_file`、`load_*_diff_data`、`generate_chat_session_title`、`compact_chat_session`、`explorer_clipboard_*`、`detach_workspace`、`delete_project`、`terminal_write/resize/close/publish`。
7. 终端：把写操作放到每个会话自己的锁里，不要在全局 `SESSIONS` 锁里阻塞；`Session::drop` 后补 `wait()`；`lsp_stop` 同理；`lsp_start` 在锁内完成检查和插入。
8. `capture_file_before_edit` 先 `metadata().len()` 再读；`load_active_diff_data` 只返回元数据，内容按需取。
9. CLI 走轻量打开：不 migrate、不 seed、不 detect。

**第三批：上限与保护**
10. 行读取加上限（改成限长读行，超过阈值丢弃或截断）；`merge_timeline_item` 改成就地 `push_str`，不要每个 token 重新拼整段并反复解析 JSON。
11. stderr 合并 / 节流，并真正写入 `stderr_output`。
12. 每次运行的事件数和输出字节加上限；`run_timeout` 给一个合理默认值。
13. 日志在运行期间也按大小轮转，并去掉每行 `flush`。
14. 保留策略和 `VACUUM`：`run_events` 的 `output` / `input_json`、`automation_runs`、`webhook_deliveries`、渠道表、`file_review_baselines`、粘贴图片目录。
15. 网关代理改成每连接一个线程（或有界线程池），给写加超时，并发限制；stream 早期错误时 kill + wait curl。
16. Git：`status` 合并成更少的进程，加 `--no-optional-locks`，输出加上限，给 `clone` 加超时和 `GIT_TERMINAL_PROMPT=0`。
17. 文件监听：不要递归监听 `node_modules`、`target`、`.git/objects`（在注册时就跳过，而不是在回调里过滤）。

---

## 7. 没有覆盖到或没有核对的

- **没有做任何运行时性能录制或内存分析**，§2 里的内存量级都是按代码推算。
- `transform.rs`（2451 行）、`hook_runtime.rs`、`ops/*`（除 `ops/agent.rs`）、`cli.rs`（除 `ensure_shim` 与 `open_file`）没有深读。
- `commands.rs:1366`、`1585`、`1664` 的 `runtimes(&db)` 是否都在同步命令里没有逐个确认（`1294` 已确认）。
- “终端僵尸进程”“管道不关闭让运行永远进行中”“权限等待延迟收尾”“MCP 孙进程孤儿”都是按机制推断，没有复现。
- 日志里 `bootstrap` / `list_runtime_models` 的 SLOW 记录与 `detect()` 的关联是机制吻合，不是直接证明。

---

## 8. 补充审计：上一轮没覆盖的部分

这一节补读了 §7 里列出的未覆盖文件。**[已核对]** 是我或 agent 读过代码的，**[推断]** 是机制推断。

### 8.1 这一轮读过 / 仍然没读的

- 读过：`transform.rs`（针对性读了 `rewrite_request`、`rewrite_response`、`synthetic_sse`、`attach_tool_names`、`flatten_responses_namespaces`，不是逐行）、`hook_runtime.rs`（40-300 行加 grep）、`ops/automation.rs`、`ops/task.rs`（`settle_finished`、`list`）、`ops/agent.rs`、`ops/squad.rs`、`ops/chat.rs`（`compact_history_text`）、`ops/inbox.rs`、`automation.rs`、`cli.rs`（`execute` 头部加 grep）、`webhook.rs`、`files.rs`（`delete_entry`、`read`）、`channel/{router,outbound,supervisor}.rs` 节选、`feishu/ws.rs` 与 `mod.rs` 节选、`protocol.rs` 的 `ByteReceiver`；另外我自己读完了 `commands.rs` 里的 `send_message` 全路径。
- **仍然没读（只 grep 或完全没看）**：`builtin_agents/**`、`builtin_skills.rs`、`merge_flow.rs`、`git_merge.rs`、`mcp_probe.rs` / `mcp_oauth.rs` / `mcp_tools_cache.rs` 的主体、`workspace.rs`、`resources.rs` 的主体、`catalog.rs`、`native_pick.rs`、`usage.rs`、`session.rs`、`models.rs`、`ops/{project,run,session,skill}.rs`、`channel/{store,commands,secret}.rs`、`feishu/api.rs`、`transform.rs` 的其余部分、`executor.rs` 1800 行之后的读流路径。

### 8.2 崩溃 / panic 风险

- **网关代理生产代码里没有 `.unwrap()` / `.expect(`**：`gateway_proxy/{mod,streaming,transform}.rs` 里所有命中都在 `#[cfg(test)]` 里（`transform.rs` 2306 行之后、`mod.rs` 659-757、`streaming.rs` 632-671）；字符串切片和索引都有 `min` 边界保护，JSON 访问都走 `.get()` / `.as_*`。[已核对]
- **但整个 `src` 里没有任何 `catch_unwind`** [我已 grep 核对]：`multara-gateway-proxy` 线程（`mod.rs:132-146`）内联执行 `handle_request`，`recv_timeout` 循环没有重启逻辑。**今天没找到触发 panic 的路径，但将来任何一个 panic 都会让这个运行的所有请求永远无响应**（运行会挂住，直到超时或手动停止）。建议至少在 `handle_request` 外包一层 `catch_unwind` 并返回 502。
- `executor.rs:356`、`1389`、`protocol.rs:33` 的切片都是安全的；`executor.rs` 1800 行之后的读流路径只做了 grep，没有逐行检查。

### 8.3 其它轮询 / 睡眠循环

| 位置 | 间隔 | 说明 |
|---|---|---|
| `automation.rs:11-12` | 30 秒 | `automation_tick`；上一轮我写成了 2 秒，**更正**：2 秒的是 `executor::start_poller`，自动化 tick 是 30 秒 |
| `channel/supervisor.rs:25-27` | 3 秒 | 每次 `lock_db` 加解析安装配置；重试退避以 1 秒为步长、上限 60 秒 |
| `channel/router.rs:492-493` | 3 秒 | 每个新的待处理批次一个睡眠线程，是防抖不是轮询 |
| `channel/outbound.rs:360` | 200ms | 恢复循环里每个待处理会话睡 200ms，再各起一个线程 |
| `mcp_probe.rs:149/324/489/527`、`mcp_tools_cache.rs:162` | `recv_timeout` 循环 | 只 grep 到，没有读 [推断] |
| `runtime.rs:714` | 8 秒 | `detect()` 里 `recv_timeout`，就是版本探测超时 |

### 8.4 新发现

1. **`settle_finished` 每次列任务都开一个写事务（中）[已核对]**：`ops/task.rs:400-436` 跑两条 UPDATE，每个进行中的任务做 `NOT EXISTS (SELECT 1 FROM agent_runs WHERE session_id=…)` 加 `ORDER BY COALESCE(completed_at, created_at) DESC LIMIT 1` 子查询，`agent_runs.session_id` 无索引，每行一次全扫（有一个还带排序）。`commands.rs:4820` 的 `list_tasks` 每次打开看板都会调用，它还是同步命令，在主线程且持锁；CLI 的 `task list`（`cli.rs:111`）也走这里。**即使没有任何匹配，它也是一次写事务。**
2. **`ops::agent::save` 里持锁调 `detect()`（中）[已核对调用；锁来源为推断]**：`ops/agent.rs:137` 的 `runtime::detect().iter().any(...)` 只有在 `existing_runtime` 与新的运行时不同时才会走到（已验证短路条件），此时会起登录 shell，最长 8 秒；传进来的 `db: &Connection` 来自调用方的锁，所以其他所有数据库用户都被挡住。触发：新建智能体或更换智能体的运行时。
3. **`ByteReceiver` 二次方 drain（低）[已核对]**：`protocol.rs:20-36` 每次读完 `self.1.drain(..n)`，一个大块经小缓冲逐段消费时是 O(块大小 × 读取次数)；`BridgeReader::read`（`streaming.rs:391`）还是逐字节从 `VecDeque` 弹出。单次廉价，几 MB 的响应体按 8~32KB 一片读时会累积。
4. **`transform.rs` 整体克隆（低）[已核对]**：`rewrite_response`（`:113`）在解析前 `body.clone()` 整个上游 JSON，同格式直通时这份克隆白做；跨格式时 `parse_openai_parts` 等对工具 JSON 克隆（`:196`、`:486`、`:1450`）；`clean_schema(tool.parameters.clone())`（`:779`、`:857`、`:1885`）每个请求克隆每个工具的 schema，50 个以上的 MCP 工具时有一定成本；base64 图片约 2~3 份拷贝，线性。**没有找到对消息数组的 O(n²)**，`attach_tool_names` 是 O(n)（用 HashMap）。`synthetic_sse`（`:119`）是 `#[allow(dead_code)]`，没人调用。
5. **`ops::agent::list`、`ops::squad::list` N+1（低）[已核对]**：每行一次 `agent_resources` / `squad_members` 查询，`prepare` 在循环里，在主线程持锁时运行，`builtin_agents::miku_system_instructions` 每次调用也会跑一次 `agent::list`。
6. **启动时的 `backfill_results`（低）[查询已核对，索引状态为推断]**：`database.rs:602`，`s.id LIKE 'automation-run:%'` 加相关 `NOT EXISTS`（tasks、inbox_items），每次启动一次扫描。
7. **渠道线程暴增（低）[已核对]**：`feishu/mod.rs:161` 每个事件一个线程，`outbound.rs:362` 每个待处理会话一个线程（错开 200ms），`router.rs:492` 每个新 key 一个睡眠线程；`router.rs` 有约 15 处、`outbound.rs` 约 11 处 `lock_db`。一阵消息会带来一阵线程和数据库锁争用，不是无界内存。
8. **`compact_history_text`（低~中）[已核对]**：`ops/chat.rs:175` 加载会话全部消息、每条截到 1200 字，`ORDER BY created_at, rowid`，`messages` 无索引，全扫加排序。
9. **`files::read_file`（低~中）[已核对]**：`files.rs:211` `fs::read` 最多 128MB，再解码；UTF-16 解码还会多一份拷贝；跑在调用它的线程上，同步命令就是主线程。
10. **`delete_entry` 的符号链接（待确认）[部分已核对]**：`files.rs:398-409` 先 `resolve_existing`（canonicalize）再 `remove_dir_all`。标准库的 `remove_dir_all` 不跟随树内的符号链接，但如果顶层路径本身是指向目录外的符号链接，canonicalize 后会删除**真实目录**。`resolve_existing` 是否拒绝根目录之外的符号链接，我没有读。这条是安全相关的待确认项，不是卡顿问题。

### 8.5 核对后确认没问题

- **`hook_runtime.rs`**：不是“每次工具调用都跑的钩子”，而是给 `commands.rs` 的提交 / 拉取 / 推送做的 git hook 与 worktree 运行时预检；自己不起子进程（grep 无 `Command::new`），只在提交、拉取、推送时跑，`link_worktree_runtime` 只做 `read_dir(.husky)` 和文件比较，很便宜。
- **`cli.rs`**：直接 `open_file` 读库，**没有 HTTP、socket、sleep、重试循环**；唯一的重开销是我们已经知道的 `open_file`（migrate + seed + `detect()`）和 `runtime list`（`cli.rs:376`，`detect()`）。
- **`webhook.rs`**：请求体 `.take(MAX_BODY_BYTES+1)` 限制，在新线程里处理。
- **自动化 tick**：一次查询；锁很短；`execute_scheduled_automation` 在 tick 线程上**串行**执行（不是每个自动化一个线程），所以一次执行阻塞，后面到期的都要等（`execute_automation_source` 没读）。
- **`send_message` 全路径 [我已读完]**：`send_message` 是 `async` + `spawn_blocking`（`commands.rs:3768`），里面只有附件的 `canonicalize` / `metadata`（最多 10 个，每个 ≤25MB 的**大小检查**，不读内容）、一个数据库事务（插入 `agent_turns` / `messages` / `message_attachments`）、`dispatch`（数据库）、然后释放锁再通知。**没有文件内容读取、没有子进程、没有网络**。它会拿 `project_locks`（每个关联项目一把互斥锁，整个函数期间持有），所以并发的两次发送、或同一项目上的 `create_task` / `update_task_status` 会串行；这段时间很短。结论：发送路径是干净的。
- **飞书 WS**：`parse_frame` 有 `MAX_FRAME_BYTES`（8MiB）上限，读超时 2 秒，ping 间隔限制在 20~300 秒，重连退避 1→60 秒。

### 8.6 之前标成“没核对”的两项，现在核对了

- **Git 命令是否异步**：`commands.rs` 里 `pub async fn git_*` 有 23 个；同步的 `git` 相关命令只有 `check_git_executable`（`commands.rs:2608`）。所以 Git 的重操作基本都不在主线程。
- **`runtimes(&db)` 的三个调用者**：`commands.rs:1366`、`1585`、`1664` 分别在 `configure_runtime`（`1350`）、`save_custom_runtime`（`1516`）、`delete_custom_runtime`（`1638`）里，**都是同步 `pub fn`** [已核对]，所以和 `detect_runtimes` 一样，在主线程持锁调 `detect()`。

### 8.7 补充后新增的建议

在 §6 的第二批里加两条：

- 给网关代理线程加 `catch_unwind` 并返回 502，避免一次 panic 让整个运行的所有请求挂死。
- `settle_finished` 不要放在 `list_tasks` 里每次都跑：改成在运行结束（`finish`）时结算，或者至少只在存在可结算任务时才开事务；同时补 `agent_runs(session_id)` 索引。

---

## 9. 第二轮补充：剩余文件读完之后

这一轮读完了前面标成“没读”的文件。**[已核对]** 是我或 agent 读过代码的；**[推断]** 是按机制推断。

### 9.0 这一轮最重要的发现不是卡顿，是数据安全

**符号链接会让删除 / 重命名 / 写入作用到工作区之外 [已核对：`resolve_existing`、`delete_entry`、`rename_entry`、`write_or_create` 的代码我都读过，没有任何一处检查路径是否仍在根目录内]**

`files.rs:556-563`：

```rust
pub(crate) fn resolve_existing(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = normalize_relative(relative_path)?;
    let target = root.join(relative).canonicalize()...;
    Ok(target)          // 没有 starts_with(root) 检查
}
```

`normalize_relative` 拒绝了 `..`、绝对路径和盘符前缀，所以**字面上的路径穿越**是堵住的；但**符号链接**没有堵：

- **删除**：工作区里有 `link -> ~/重要目录`，用户在文件树里删除 `link`。`delete_entry`（`files.rs:398-410`）拿到的是 canonicalize 之后的**真实目录**，`target != root` 通过，`target.is_dir()` 为真，然后 `fs::remove_dir_all(target)` —— **删掉的是链接指向的外部真实目录，而不是链接本身**。
- **重命名**：`rename_entry`（`files.rs:384-396`）的源同样走 `resolve_existing`，会把外部目标整个**移动**进工作区。
- **写入 / 新建**：`write_or_create`（`files.rs:324-340`）先 `create_dir_all(parent)`（跟随符号链接，会在根目录外创建目录），再 canonicalize 并写入，没有任何包含检查；`create_file` 用 `target.exists()` 判断，对**悬空符号链接**返回 false，之后 `fs::write` 会顺着链接在目标位置创建文件。`create_directory` 和 `rename` 的目标同理。
- 读取（`read`）和 `reveal` 也会跟随符号链接读到根目录之外（低危泄露）；`read_absolute` / `list_absolute`（`files.rs:223`、`303`）本来就接受任意路径，是设计如此，但意味着如果 WebView 被攻破就能读任意文件。

**现实里会不会碰到**：pnpm / monorepo 的 `node_modules`、`link:` 依赖、指向别的仓库的符号链接都很常见，用户在文件树里对这类条目点“删除”就可能触发。我没有实际复现，也没有在文件树里验证右键删除是否会先做别的检查，但函数本身没有任何保护。

**建议（小改动、风险低）**：`resolve_existing` 之后加 `if !target.starts_with(root) { 拒绝 }`；对删除先用 `symlink_metadata` 判断，如果是符号链接就只 `remove_file` 链接本身；`write_or_create` / `create_*` 在 `create_dir_all` 之前先逐级检查，或者创建后再校验 canonical 父目录仍在根内。

另有两条路径安全问题（来自 §8 之外的 `resources.rs` / `code_index.rs`）：

- **S1（中）[已核对 `find_skill_dir`；`percent_decode` 行为为排查所得]**：`resources.rs:875-886` 的 `find_skill_dir(root, preferred)` 直接 `root.join(name)`，`name` 来自 URL 片段（经 `percent_decode`，`%2e%2e`、`%2f` 都会被解码）。构造 `github.com/o/r/tree/main/..%2f..%2f..` 之类的 URL 可以越出临时克隆目录，指向本地任意含 `SKILL.md` 的目录，然后 `install_skill_dir` 把那个本地目录整个复制进技能目录。需要拒绝 `..` 和绝对路径片段。
- **S2（低）[排查所得]**：`code_index.rs:231-238` 的 `resolve_package` 把 `package.json` 依赖键拼进 `node_modules/<name>`，`../..` 可以跳出；只读 `.d.ts` 和 `package.json`。
- 渠道密钥 `keychain_set` 把应用密钥放在 `security add-generic-password -w <secret>` 的**命令行参数**里（`channel/secret.rs:36-45`），本机其他进程用 `ps` 能看到 [排查所得]。
- `http_client` 的 `--location` 会把自定义头（如 `X-Api-Key`）带给重定向目标（低）[排查所得]。

### 9.1 回答你上一轮问的两个问题

**一次发给 CLI 的提示词最大多大，历史有没有上限 [排查所得，`conversation.rs` 行号已对照]**

- 每轮提示词 = 简介 + 历史 + “用户任务：” + 本次任务（`conversation.rs:57-67`，`executor.rs:981` 处拼接）。
- 历史**有上限**：`HISTORY_CHAR_LIMIT = 60,000` 字符、每条消息 `ENTRY_CHAR_LIMIT = 8,000` 字符（`conversation.rs:14-15`、`346-353`），保留最新、塞得下的。注意是**字符数不是字节数**，6 万个汉字约 180KB。
- 如果有 CLI 的恢复 id 且该运行时能通过 CLI 恢复，历史为空（`conversation.rs:275`）；否则每轮都会重发被截断后的全部历史。
- **没有上限的部分**：本次任务（用户文本加附件 JSON，`commands.rs:3980-3995`）、`compact_summary`、回滚说明、小队成员提示词里嵌入的**完整队长输出**（`executor.rs:1991-2000`）、技能文本（每个 128KB、总共 256KB，`executor.rs:2390`）。
- 实际量级约 6 万字符历史 + 最多 256KB 技能 + 简介 + 不限长的任务，约 0.3~0.5MB，粘贴大段内容会更大。
- **[推断]**：通过命令行参数传提示词的运行时（`replace_assignment`，`executor.rs:1024`）可能撞 `E2BIG`（Linux 单个参数约 128KB，Windows 整条命令行约 32K 字符），结果是“启动运行时失败”，没有截断回退。哪些运行时走 argv（`runtime::prompt_via_stdin`）没有读过，未验证。

**`list_runtime_models` 为什么慢（2.5~3.5 秒）、多久触发一次 [排查所得]**

- 只有 `cursor` 运行时真的做事：`cursor-agent --list-models`（Node / Electron 启动 + 网络），代码注释也写了“cursor 约 3 秒”（`runtime_models.rs:30`）；其它运行时返回静态数据。命令是 `spawn_blocking`（`commands.rs:1308`），**不阻塞主线程**，只占用一个阻塞线程池线程约 3 秒。
- 缓存有缺口：只有**非空成功**才缓存，10 分钟（`runtime_models.rs:82-84`）；空结果（没登录）和错误**不缓存**，所以没登录或失败的 cursor 每次挂载 / 切换运行时都重新起一次 CLI；**没有并发去重**，两个同时挂载的选择器或快速切换会并行起 N 个 `cursor-agent --list-models`；缓存只在内存里，每次启动都要付一次 3 秒。
- 前端触发：`RuntimeModelSelect.vue:46` 的 `watch(() => props.runtimeId, load, { immediate: true })`，每次挂载和每次 `runtimeId` 变化都触发；用在 `AgentDetailView.vue:589`、`CollectionView.vue:464`、`:563`。
- 超时路径（中）：`run_with_timeout`（`runtime_models.rs:112-136`）起一个线程跑 `command.output()`，20 秒后**放弃但不杀子进程**，线程和子进程会泄漏到 CLI 自己退出。

**应用退出时哪些子进程会活下来 [已核对：我 grep 了整个 `src`，没有 `ExitRequested`、`on_window_event`、`RunEvent`、`killpg`]**

- `lib.rs` 没有任何退出处理，`.run(...)` 也没带回调；唯一的退出钩子是 Windows 的 `win_shutdown`，只调 `process::exit(0)`。`terminal::Session`、`GatewayProxy`、MCP 连接的 `Drop` 在退出时**根本不会执行**（`SESSIONS` 是 static，`process::exit` 不会析构）。
- 会残留的：
  - **智能体运行的 CLI**（claude、codex、cursor 等）：用 `process_group(0)`（`executor.rs:1092`，已核对这是整个 `src` 里唯一的 `process_group`）放在独立进程组，不会收到终端 HUP；stdin 为 `Stdio::null()` 的 codex 会一直跑完；用管道 stdin 的 claude / cursor 通常在 EOF 时退出但不保证；它们的孙进程（`npx` / `uvx` 的 MCP 服务器、bash 工具、开发服务器）即使 CLI 退出了也会活着。
  - **所有“只杀直接子进程”的清理**：整个仓库里的 kill 都是 `Child::kill()`（单个 pid）：`executor.rs:96` 的 reap、`mcp_probe.rs:347/548`、`terminal.rs:25`。所以点“停止”，`npx → node server` 这种链条里的孙进程也会成为孤儿。
  - PTY shell 在 master 关闭时收到 SIGHUP，多数会死；`nohup` / `setsid` 的作业会活。LSP 服务器只有在 stdin EOF 时才会退出 [推断]；正在进行中的 MCP 探测子进程会活到超时。
  - 线程、`tiny_http` 网关 / webhook 服务器和 OAuth 回调随进程消失。

### 9.2 新发现（按影响排序）

**高**

1. **supervisor 在孙进程握着管道时永远挂住（高）[`join` 已核对，触发为推断]**：`executor.rs:1307-1312`，子进程退出后对 stdout / stderr 的读线程 `join()` 没有超时。CLI 退出但它起的 MCP 服务器或后台开发服务器继承了管道，EOF 永远不来，运行一直是“进行中”，占着并发槽，`run_timeout` 也不再被检查（循环已退出），`reap_orphaned_runs` 也救不了（`supervisor_alive` 仍为 true）。只有手动 `stop()` 能救，因为它 `kill -KILL -pgid`。正常退出路径从不调用 `kill_child`，所以存活的进程组成员会泄漏；`generate_text`（`executor.rs:482-483`）有同样的 `join`，而且没有 `process_group`。
2. **仍然是同步命令的重操作（高）**：`compact_chat_session`（`commands.rs:4742`，最长 90 秒）、`generate_chat_session_title`（`:3363`，最长 8 秒）、`stop_run`（`:4421`）、`read_workspace_file` 等文件命令（`232-310`，最大 128MB）、`delete_workspace_entry`（同步 `remove_dir_all`，比如 `node_modules`）。都不是 `off_main`。
3. **`detect_runtimes` 持锁在主线程起登录 shell（高）**：已在 §0、§1.1 里，这里补充一个触发点：前端“重新检测”按钮（`CollectionView.vue:300`、`:345` → `store.refreshRuntimes`）。每次 `detect()` 还会对约 27 个规格跑 `resolve_runtime_command`，未命中（`not_found`）从不缓存，每次重复 `which` 加 15 个目录搜索加 nvm 的 `read_dir`（便宜，但累积）；`cli_version` 按路径永久缓存，升级后版本号要重启才刷新。
4. **`detach_workspace`、`delete_project` 持锁在主线程跑 git worktree 删除（高）[已核对签名为同步 `pub fn`]**：`commands.rs:1020-1050`、`1096-1105`，持有 `lock_db()` 调用 `workspace::drop_worktree_checkout` / `workspace::remove`，执行 `git worktree remove --force`、`branch -D`、`worktree prune`（全是无超时的 `.output()`），复制模式下还要对整棵树 `remove_dir_all`；`delete_project` 对项目里每个工作区循环做一遍。触发：删除一个有大型 worktree 的项目，界面冻结，所有其它命令都卡在数据库锁上。
5. **`scan_workspace_candidates` 对每个子目录起 2~3 个 git 进程（高）[已核对签名为同步]**：`workspace.rs:133-181`（`discover`）每个非点号子目录跑 `find_git_root`，git 子目录还跑 `git_branches`（`for-each-ref` 两个范围，stdout 无上限）和 `current_branch`。选 `~/Documents` 或 `~/code` 这种有 200 个子目录的目录，就是 200~600 个 git 进程，在主线程，无超时，无数量上限。
6. **`import_skill_from_folder` / `transfer_resource` 整树复制（高）[已核对签名为同步]**：`commands.rs:2015`、`2814`；`resources.rs:659-662, 819-833, 1584-1600` 的 `copy_directory` 没有深度、大小、文件数上限，也不跳过 `.git`、`node_modules`；`find_skill_dir` 在所选文件夹本身有 `SKILL.md` 时返回根目录，所以用户选了一个带 `SKILL.md` 的仓库根目录就会在主线程复制数 GB。遇到任何符号链接才报错，此时复制已经做了很多，留下半个目标目录，`install_skill_dir` 失败时不清理。
7. **`resources::scan()` 没有缓存，每次调用重新遍历所有 CLI 技能根目录并解析所有 MCP 配置（高）[排查所得]**：每个技能一次 `read_skill_state()`（每个技能都读并解析一次 `skill-state.json`）、一次整个 `SKILL.md` 读取；每个 Cursor MCP 条目打开一次 `state.vscdb`（SQLite）、一次文件 stat、一次 `mcp_vars` 存储读取。`scan_resources` 和 `list_resource_sources`（同步）都持锁做完整文件系统扫描；而且 `scan()` 还被用来“按 id 找一项”：`resource_path`、`skill_directory`、`transfer`、`ops::skill::get`、`set_skill_enabled`、`delete_skill`、`reveal_resource`，`transfer_resource` 甚至扫两遍；`executor.rs:2357` **每次启动运行都扫一次**。

**中**

8. **`steer()` 持有全局 `active_runs` 互斥锁做阻塞的 stdin 写（中）[排查所得]**：`executor.rs:549-575`。子进程不再读 stdin、管道写满时，`stop()`、reap、`can_steer`、`resolve_permission` 和每个 `spawn_run` 的插入 / 移除都会阻塞在这把锁上，整个执行器停摆。
9. **stdin 写线程的 `join()` 在 supervisor 循环之前（中）[排查所得]**：`executor.rs:1239-1272`。子进程不读大 stdin 载荷时 `write_all` 阻塞，超时不生效，只有外部 `stop()` 能杀。相关竞态：`stop()` 发生在准备阶段（`abort_stopped` 只在 `902`、`929` 检查）与 `active_runs` 插入（`1187`）之间时，`stop()` 找不到子进程直接 `finish("stopped")`，但 `spawn_run` 仍然会启动 CLI 和网关代理，直到循环的第一个 tick（`1284`）才被杀，而此前还要先等写线程 `join`。
10. **supervisor 每 120ms 查一次数据库（中）[排查所得]**：`executor.rs:1284`、`743-748` 每个运行每 120ms 调一次 `is_stopped`（拿全局锁、一次查询），给所有其他数据库用户增加争用；停止或超时后 `kill_child` 每个 tick 还会 fork 一个 `kill` 进程直到子进程被回收（`750-772`）。这和我上一轮的节流（读流线程 500ms 一次）是**不同的路径**：读流线程已经节流，supervisor 循环没有。
11. **`conversation::prepare()` 持锁加载整个会话历史（中）[排查所得]**：`conversation.rs:269-353`，`history_block` 加载会话里**所有**消息，`clip`（`362-369`）先把每条消息 `chars().collect::<Vec<char>>()` 再走 6 万字符预算，所以每次启动运行的代价是整个会话大小，且持有数据库锁。
12. **`generate_text` 先同步写 stdin 再启动读线程和超时循环（中）[推断]**：`executor.rs:445-452`；提交信息提示词会嵌入整个暂存区 diff（`143-160`，没看到上限），大 diff 加“先写后读”的子进程可能死锁，没有超时；走 argv 的话也可能 `E2BIG`。
13. **`files.rs` 大文件内存与树遍历（中）[排查所得]**：`read()` 最多 128MB（可编辑 64MB），`fs::read` 后 `decode_text` 再复制一份，再加 128MB 的 JSON 走 IPC；媒体预览 base64 最多 32MB 又是一份；`list_tree` 超过 5,000 个条目直接 `Err`（`:146`），而 `node_modules` 不在 `EXPLORER_EXCLUDE`（`:19`）里，大仓库很可能总是报错；每个条目一次 `symlink_metadata`、一次 `read_link`、一次 `canonicalize`，再加排序键的 `fs::metadata`；`walk_files` 遇到第一个不可读目录（`?`，`:107-110`）整个遍历中止。
14. **渠道密钥每次都起 `security` 子进程，且持锁、在主线程（中）[已核对 `keychain_get` 起子进程和 `list_channel_installations` 同步；频率为推断]**：`channel/commands.rs:93-100`，`list_channel_installations` 同步命令，持 `lock_db()` 对每个安装调 `secret::load`，macOS 上是 `/usr/bin/security find-generic-password`（无超时），每个 20~100ms；钥匙串被锁或弹授权框就会阻塞主线程和全局数据库锁。`save_channel_installation` 同理；`test_channel_installation`（`:264-282`）是同步命令，主线程做 HTTP（`bot_info()` 30 秒超时，再加租户 token 的 30 秒）。此外 `api_for`（`feishu/mod.rs:15-31`）**每次**都 `secret::load`，`outbound.rs`、`router.rs`、`feishu/mod.rs` 里每次发送 / 回复一次 `security` 进程；如果卡片是流式更新，就是子进程风暴。
15. **用量解析每行完整 JSON 解析（中）[排查所得]**：`usage.rs:93-97` `parse_line` 对**每个** stdout 行 `serde_json::from_str::<Value>`（完整 DOM 分配，包括巨大的 assistant / tool_result 行）；`executor.rs:1467-1468` 随后 `persist_run_usage(&acc.finalize())`，`finalize()` 克隆 `TokenUsage` 并计算价格（含 `format!` 分配），**每条带用量的行一次数据库写**；claude 每条 assistant 消息都带 `message.usage`，codex 的 `token_count` 事件频繁，而 `extract_usage` 最后一个回退 `Some(value)` 让任何带 `input` / `output` 字段的行都匹配；同一行还被 `persist_from_line`、`normalize_runtime_line` 各解析一遍，共约 3 次。建议先用 `line.contains("usage")` 之类的廉价预过滤，并且用量最多每隔几秒或在结束时才写。
16. **MCP 工具缓存整体重写加失败探测反复排队（中）[排查所得]**：`mcp_tools_cache.rs:69-77`、`104-125`，`persist_locked` 每次 `put` 都 `to_string_pretty` 整个缓存（每次探测日志最多 20KB 加工具列表），持有 `inner` 互斥锁，`list()` / `get()` 被阻塞；写入不是原子的（`fs::write`），中途崩溃会截断；`schedule()` 只跳过“新鲜且成功 / 有工具”的条目，失败或需要授权的探测每次 `refreshInBackground`（`stores/mcp-tools.ts:49-65`）都会重新排队，每次失败都起一个 `npx`，最长 40 秒启动超时，反复发生。
17. **MCP 探测清理只杀直接子进程（中）[`kill` 已核对；泄漏为推断]**：`mcp_probe.rs:345-350`、`546-551`，`npx -y pkg` / `uvx` 的 node / python 孙进程成为孤儿；`crate::process::command` 没设 `process_group`，每次失败或超时的探测都泄漏一个服务器进程，并随 16 反复发生；每次探测还起 2 个读线程，依赖 kill 才会退出。
18. **权限等待阻塞整条读流（中）[排查所得]**：`protocol.rs:52-66`，`PermissionGate::decide` 在 `rx.recv_timeout(300s)` 里阻塞协议读线程，等待期间**完全不读智能体输出**，一个没人点的权限提示会让整个运行卡 5 分钟；`pending` 映射超时后不清理（小泄漏）；`call()`（`:213-247`）没有截止时间，ACP 智能体不回应 `initialize` / `session/new` 时一直阻塞在 `read_line`；`read_line` 没有行长上限。
19. **`git_merge` 的冲突文件名未加 `-z` 导致误判（中，正确性问题）[已核对 `conflict_files` 没有 `-z`]**：`git_merge.rs:380-388` 用 `git diff --name-only --diff-filter=U`，默认 `core.quotepath` 下中文等非 ASCII 文件名会被加引号并转义；`files_with_markers`（`:400-410`）随后 `fs::read(Path::join(file))` 失败，`unwrap_or(false)` 把它当成“没有冲突标记”，`commit_resolution` 的 `git add -A` 就会**把带冲突标记的内容提交**。触发：冲突文件名含中文。
20. **未加超时的 git 子进程（中）[排查所得]**：`git_merge.rs` 里的 `plan_merge`、`outlook`（`merge-tree`）、`start_conflict_worktree`（`worktree add --detach`、`merge --no-ff --no-commit`）、`incorporate_remote`（`git merge --no-edit`）、`is_ancestor`，以及 `workspace.rs` 的 `create_worktree`，全是无超时的 `.output()` / `.status()`；`worktree add` 会跑 post-checkout 钩子（husky），钩子挂住命令就挂住。它们都在主线程之外、不持数据库锁，只丢一个阻塞线程；只有 fetch / push / commit 有 180 / 120 秒的界限。
21. **`git_merge_preview` 约 20~25 次串行 git（中）[排查所得]**：`merge_flow.rs:278-345`，`git status --porcelain`（`:288`）只为判断是否为空却捕获全部 stdout，没有 gitignore 的 `node_modules` 会很大；`compare()` 以相同参数被调用 2~4 次（直接调一次，`outlook` 里又一次）；`resolve_target` 反复调 `remotes()` 和 `rev()`。每次切换目标都会重来。
22. **`http_client` 响应体先落盘再读入内存才检查上限（中）[排查所得]**：`http_client.rs:102-117`，8MB 上限在 `fs::read(body_file)` **之后**才检查，没设 `--max-filesize`，只有 `--max-time`（默认 30 秒）兜底；快速服务器可以在“响应过大”检查前把数百 MB 写盘再读进内存；每个请求 2~3 个临时文件加一个 `curl` 进程；`request_stream` 把 stderr 接管道但从不读。

**低**

23. `catalog.rs:33-42`：每次技能搜索都拉取 `anthropics/skills` 的完整递归 git tree，无缓存无 ETag，未认证的 GitHub 每 IP 每小时 60 次，403 返回 `Err` 而不是回退列表，`truncated` 标志被忽略（低~中）。
24. `workspace.rs:359-378` `copy_directory`（隔离模式 `copy`）：整个源目录复制，无大小上限，不跳过 `node_modules`，遇到第一个符号链接（如 `node_modules/.bin/*`）才报错，复制中途中止，`prepare` 再 `remove_dir_all` 清理残留；在 `spawn_blocking` 里（低~中）。
25. `builtin_skills.rs:94-102` `materialize`：**每次启动运行**都用非原子的 `fs::write` 重写 7 个 `SKILL.md`（`executor.rs:968`），语言变化时还在 `lock_db` 下（`commands.rs:2602`）；并发启动会截断重写同一批文件而 CLI 可能正在读；只在内容变化时才写就能避免。
26. `merge_flow.rs:435-584`：worktree 先于数据库插入创建，两者之间崩溃会留下没人追踪的 `merge-<id>` 孤儿 worktree；`delete_project` 对每个工作区行（含会话级合并工作区）调 `workspace::remove`，但 `merge_flow::delete_for_project` 只删 `merge_sessions` 行。
27. `session::project_locks` 的 `HashMap` 条目从不移除（小泄漏）。锁顺序（`project` → `lock_db`）在我读到的所有位置一致（`commands.rs:2968`、`3014`、`3871`），**没有发现倒置**；但同步命令会在主线程上等 project 互斥锁，而这把锁持有期间又在等 `lock_db`（`send` 里还有 `executor::can_steer`），所以慢的数据库持有者会传导到主线程。
28. `executor.rs:76-113` `reap_orphaned_runs` 只 `process.kill()` 领头进程，不杀进程组，孙进程泄漏；`active_runs` 插入（约 `1187`）之前发生 panic 会让运行在数据库里一直是 `running`，直到下次启动的 `recover_interrupted_runs`（`database.rs:608`）才修复 [推断]。
29. `executor.rs:3359-3388` `truncate_and_redact`：先对整个字符串脱敏再截断（逐行 `to_ascii_lowercase`、`collect`、`join`，最后才 `take(max_chars)`），超长单行有 3 次以上的 O(n) 分配，`text_at` 对多个指针各跑一遍，`sanitize_value` 还要遍历整棵树；和已知的无上限 `lines()` 叠加。
30. 其它小项：`runtime.rs:728-733` 与 `mcp_oauth.rs:455` 每次调用都 `Regex::new`（应放 `OnceLock`）；`runtime.rs:701-714` 版本探测线程在 8 秒超时后被丢弃而子进程不杀，挂住的 CLI 每次扫描泄漏一个线程和进程（`cli_version` 缓存了 `None`，所以每个路径只发生一次）；`mcp_oauth.rs:35-48` 过期时同步 `curl` 刷新（`--max-time 20`），并发调用方没有互斥，非原子 `fs::write`，轮换刷新令牌时可能互相失效 [推断]；`authorize`（`commands.rs:2696-2715`）在 `spawn_blocking` 里最多阻塞 180 秒（命令是异步，但占一个线程池线程，且没有取消路径）；`ops::project::get`、`ops::skill::get` 各跑一次全量列表 / 全量扫描再在 Rust 里过滤；`channel/store.rs` 的 `prune_inbound_dedup` 是否被调用没验证（7 天截止已写好，去重表可能无限增长）[推断]；`feishu/api.rs` 的 `tenant_token` 没有单飞，过期后并发调用方都会去 POST 一次。

### 9.3 核对后确认没有问题

- **网关 `transform.rs` 没有栈溢出 / 环 / panic 风险**：`clean_schema` 是对 `serde_json::Value` 的线性递归，没有 `$ref` 展开，所以没有指数爆炸；`Value` 不可能有环；深度受 `serde_json` 默认 128 层解析限制，整个源码树里没有 `disable_recursion_limit`；没有对请求数据做 `unwrap`、下标或切片，数据 URL 解析用 `strip_prefix` 和 `split_once`。每次请求是线性代价，带一些拷贝：每个工具 schema 克隆约 3 次，base64 图片 / 文档约 3~4 份，`rewrite_response`（`:113`）无条件 `body.clone()`。
- **`active_runs` 清理**覆盖了我能找到的所有退出路径（`1244`、`1255`、`1313`、`stop`（`633`）、`reap`（`101`））；`finish()` 幂等（`status IN ('queued','running')` 守卫）；`process_group(0)` 在 `1092`；停止时和循环后都会关 stdin（`644`、`1304`）；`save_terminal_message`、`dispatch_squad_members`（`already > 0` 守卫）、`maybe_resume_parent`（`open_children` 守卫）没有循环或泄漏；MCP 临时文件是 0600 的 `NamedTempFile`，闭包结束时丢弃。
- `native_pick.rs`：`spawn_blocking` + `run_on_main_thread` + 会合通道，macOS 的模态窗口按设计在主线程。`main.rs` 无内容。`models.rs` 没有 `#[serde(flatten)]`，只有 `input`、`source_meta` 和一个值映射含 `Value`，没有追踪每次 IPC 是否克隆。`code_index.rs` 有界（深度 16、3000 文件、每个 512KB、总 16MB、类型声明 4000 文件 / 24MB、不跟随符号链接、`read_limited`），一批 IPC 最多约 40MB。

### 9.4 覆盖情况

- **读完**：`executor.rs` 1-1419 与 1640-3744（含测试），只缺 1420-1639（`read_stream` 与 `PendingStream`，前面审计过）；`gateway_proxy/transform.rs` 1-2286（缺 2287-2451 的测试）；`gateway_proxy/protocol/mod.rs` 1-37；`files.rs` 1-660（缺 660-923 的语言表与测试）；`merge_flow.rs`、`workspace.rs`、`session.rs`、`http_client.rs`、`catalog.rs`、`builtin_skills.rs`、`runtime.rs`、`runtime_models.rs`、`mcp_probe.rs`、`mcp_tools_cache.rs`、`protocol.rs`、`main.rs`、`native_pick.rs`、`process.rs`、`ops/{project,run,session,skill,mod}.rs`、`channel/{store,secret,commands}.rs`、`feishu/api.rs`。
- **读了大部分**：`git_merge.rs` 1-520（共 696，缺测试模块）；`resources.rs` 1-1024 和 1370-1600，1024-1370（MCP 读写 / 保存辅助）只靠 grep 略读；`usage.rs` 1-260 加价格表尾部略读；`mcp_oauth.rs` 1-520 加尾部和测试；`code_index.rs` 1-240；`feishu/decode.rs` 1-230；`feishu/pb.rs` 1-200。
- **仍然没读**：`builtin_agents.rs`（只看了目录：5 个 `INSTRUCTIONS*.md`，共约 18KB，全是 `include_str!`）、`mcp_vars.rs`、`runtime_brief`、`runtime::prompt_via_stdin`、`protocol::drive`、`channel/mod.rs` 除开头外、`lsp.rs`（依赖前面审计者的结论）、`lib.rs` 的注册列表。
- 所有“慢”和“泄漏”仍是按代码推断的，**没有在运行中复现过**；唯一的实测数据还是 `multara.log` 里的耗时记录。

### 9.5 建议的处理顺序补充

在 §6 的基础上：

**第一批（安全 / 数据丢失，建议最先做）**
- 给 `resolve_existing` 加根目录包含检查；删除前先判断符号链接，只删链接本身；`write_or_create` / `create_*` 在 `create_dir_all` 之前校验。
- `find_skill_dir` 拒绝 `..` 与绝对路径片段。
- `git_merge::conflict_files` 等所有解析文件名的 git 输出加 `-z`，并在读不到冲突文件时按“有冲突标记”处理（失败即安全），而不是 `unwrap_or(false)`。

**第二批**
- supervisor 对读线程 `join` 加超时（例如子进程退出后再给 2~5 秒，然后对整个进程组 `killpg` 并放弃等待）；所有 kill 改成 `killpg`；`lib.rs` 加 `RunEvent::ExitRequested` 处理，退出时停止所有运行、终端、LSP、MCP 探测和网关。
- `steer()` 不要在 `active_runs` 锁里写 stdin；supervisor 循环把 `is_stopped` 改成低频或由事件通知。
- 把 §9.2 里 2、4、5、6、7、14 的命令改成异步（`spawn_blocking`），`scan()` 加缓存或按 id 查找。
- `usage.rs` 加廉价预过滤，用量落库改成按时间合并。
- MCP：探测用进程组、失败探测加退避、缓存写入改原子且不要每次整体重写。

---

## 10. 第三轮补充：把剩下没读的文件读完

这一轮覆盖了 §9.4 里“仍然没读”的几乎全部文件。标记同前：**[已核对]** 是我或 agent 读过代码，**[推断]** 是按机制推断。我自己抽查核对过的有：`atomic_write`、配对码生成与比较、`form_decode` 的字符串切片、`protocol.rs` 的 `id == 3` 判断、`lsp_send` 的同步签名、`lib.rs` 没有单实例插件、`resolve_permission` 按 `request_id` 跨所有运行查找。

### 10.0 对我上一轮改动的复核（先交代我自己的代码）

有一路 agent 专门复查了我之前加的 `PendingStream` 批处理代码（`executor.rs` 1415-1645）：

- **没有死锁**：锁顺序始终是 `pending` → 数据库（`flush` → `persist_timeline_item` → `lock_db`），没有任何地方反过来；flusher 与读线程只会互相等待，不会互锁。
- **最后一块不会丢**：流结束路径 `1569-1573` 先置 `done`、`join` flusher、再 `flush`；`break`（停止）走同一个尾部；顺序也对（只有读线程 push，换 seq 和遇到非文本事件时先 flush）。
- **但有三个我自己引入的小问题**：
  1. **stderr 流也起了一个 flusher 线程 [已核对]**：`is_error` 流只会产生 `error` 事件，永远不会 push 到 `pending`，却白白起一个每 80ms 醒一次的线程，整个运行期间都在。应只在 `!is_error` 时起，或改用 `Condvar` / `recv_timeout`。
  2. **读线程 panic 会泄漏 flusher [已核对]**：`stream_done` 只在 `1488` 和 `1569` 置位，`normalize_runtime_line`、`persist_*`、`emit` 任何一处 panic，flusher 就带着 `AppHandle` 克隆永远循环。应该用 Drop guard。
  3. **工具报错熔断路径丢用量 [已核对]**：`1487-1493` 触发熔断时，触发它的那条 `tool_result` 被丢弃，最后的 `persist_run_usage` 被跳过，flusher 也没有 `join`。
- **仍然存在的（上一轮没动、只是没修）**：合并仍是逐 token 的 O(n²) CPU 与内存拷贝（`merge_timeline_item` 每 token `format!` + `clone()` + 对整段 `serde_json::from_str`，文本以 `{`、`[` 或数字开头时会一直解析到末尾，回复恰好是 `123` / `true` 还会被写进 `item.input`；200KB 回复约 5 万 token，量级 10~20GB memcpy）；每次 flush 仍带**完整的合并后条目**走 IPC，数据库 upsert 也重写全文，所以 IPC 和 WAL 量对长回复仍是 O(n²)，只是频率降到每秒约 12 次。
- **一个需要对照前端确认的语义**：`replaces_final_text` 一旦置位，之后每次合并 flush 都带 `replaces_final_text=true`，而 `chunk` 只是上次 flush 以来的增量；如果前端在这个标志下用 `chunk` 替换正文，输出会被截断。这是原来逐 token 路径就有的语义，不是批处理引入的；我看过前端运行中显示正文用的是时间线里的 `liveAssistantText`，运行结束后又从数据库覆盖，所以实际可见影响应该很小，但没有端到端验证。

### 10.1 数据丢失与损坏（比卡顿更值得先看）

1. **MCP 配置写入不是原子的，且会丢失 CLI 的并发修改（高）[`atomic_write` 已核对；并发频率为推断]**：`resources.rs:1561-1581`，顺序是写临时文件 → **把原文件改名成 `.multara.backup`** → 把临时文件改名成原路径。两次改名之间原路径**不存在**，此时正在写同一文件的 CLI（Claude / Cursor / Codex）会被覆盖，进程崩溃则只剩 `.multara.backup`；临时文件用默认 umask 创建，`~/.claude.json`（0600，含请求头和令牌）写回后变成 0644；符号链接的点文件（stow 管理）会被换成普通文件；没有 `fsync`，崩溃可能留下 0 字节配置。
2. **`update_servers` 整文件读-改-写，没有锁、mtime 检查或合并（高）[排查所得]**：`resources.rs:1031-1061`。`~/.claude.json` 被 Claude CLI 持续改写（历史、OAuth 账号），Multara 每次保存 / 启用 / 删除都用旧快照整个写回，读到写之间 CLI 的修改会丢。
3. **`save_mcp` 不是事务（低）**：配置写入、Cursor 状态、`rename_inputs` 三步中间失败会留下一半重命名但返回 `Err`；JSON 重写没有 `preserve_order`（`Cargo.toml:33`），键可能被重新排序；JSONC 注释经 `strip_jsonc` 回退丢失（`resources.rs:1510-1556`）；TOML 的 `to_string_pretty`（`:1052`）会丢掉 `~/.codex/config.toml` 里的全部注释并重新排版。
4. **Webhook 事件会被静默丢弃（高）[排查所得]**：`commands.rs:2356-2475` 与 `4883-5040`：
   - 同一个自动化已有 `pending` / `running` 的运行时，新运行被标成 `skipped (already_running)`，但 webhook 投递仍被置为 `dispatched` 并返回 202，**突发事件被丢掉而发送方以为成功**；
   - 投递行在 `execute_automation_source` 之前就以 `queued` 插入，若它出错，行停在 `queued`，发送方重试会命中“重复”分支（`WHERE status='rejected'`），事件**永久丢失**；
   - `execute_automation_source` 在插入 `pending` 运行之后用 `?` 返回（比如 `chat_sessions` 或 `messages` 插入失败），这个运行会一直 `pending` 到应用重启，并让该自动化之后的所有运行都变成 `already_running`。
5. **快照清理会删掉还没审阅完的回滚数据（中）[排查所得]**：`snapshot_db.rs:511-575` 的 `cleanup_old_snapshots` 对最新 30 个会话之外的会话删除快照和块状态，即使里面还有待“保留 / 撤销”的修改，旧的待审修改会悄悄失去回滚能力；它每次运行结束都在数据库锁下跑一遍全表 `GROUP BY` 和 `SUM(LENGTH(content))`（`executor.rs:1832`），启动时 `migrate()` 再跑一遍（`database.rs:600`），没有事务、没有 `VACUUM`。
6. **快照对象选错工作区、漏文件（中）[映射已核对，影响为推断]**：`snapshot.rs:102` `resolve_workspace_file` 对相对路径**总是**映射到 `workspaces.first()`，多工作区会话里快照取的是错的工作区的同名文件，真实被改的文件没有基线，撤销会恢复错的内容或什么都不恢复；绝对路径用纯字符串前缀匹配（不 canonicalize，在 macOS 大小写不敏感文件系统上区分大小写），会漏；超过 2MB 的文件和非 UTF-8 文本（GBK / Latin-1 被当成二进制）静默不做快照，撤销 / 回滚对它们什么都不做。
7. **数据库表重建迁移没有事务（低）[排查所得]**：`database.rs:731-883`（`workspaces`、`tasks`、`automations`、`diff_chunk_states`）用 `execute_batch` 且没有 `BEGIN`，`DROP` 与 `RENAME` 之间崩溃会丢表；`migrate_diff_chunk_keys` 会清空所有块状态；`session.rs:75-94` `replace_linked_projects` 先 `DELETE` 再 `INSERT` 没有事务。
8. **`mcp_vars` 密钥存储**：`mcp_vars.rs:188-200` 用 `fs::write` 创建（默认 0644）之后才 `set_permissions(0o600)`，首次创建有一个任何本机用户可读的窗口；非原子写，写到一半崩溃会截断所有已保存的输入和令牌；`read_store` 把损坏的文件当作空，下一次 `save_inputs` 就覆盖它，**这是数据丢失**。

### 10.2 安全（单列）

**高**

- **S1 频道配对码太弱，且轮换无效 [已核对生成与比较]**：`channel/commands.rs:82-90`，配对码是 6 位数字（约 20 比特），由 `uuid` 字节 `% 10` 生成；`router.rs:114-124` 的 `try_pair` 是纯字符串比较 `candidate != code`，**不过期、不是一次性、没有尝试次数限制**。配对成功的发送者角色是“owner”，可以运行智能体并**批准工具权限卡片**。另外 supervisor 的指纹（`supervisor.rs:32`）不包含 `pairing_code` / `group_enabled`，所以 `rotate_channel_pairing_code` → `reload` 不会重启正在运行的连接：连接仍持有旧的 `installation.config` 克隆，旧码一直有效到重连，`group_enabled` 的切换同样被忽略。
- **S2 MCP 令牌按服务器名发送，没有绑定 URL [排查所得]**：`mcp_vars.rs:105-114` + `mcp_oauth.rs:336-341`，`prepare()` 会给任何同名且没有 `Authorization` 头的远程条目加 `Authorization: Bearer <token>`，不要求有 `oauth` 字段；令牌文件只以 `sha256(name)` 为键，没有 URL 或所有者绑定。再装一个同名服务器（目录安装、工作区 `.mcp.json`）就会把旧服务器的令牌发到新 URL；`delete_mcp` 也不删除令牌和已存输入。`${env:NAME}` 展开（`mcp_vars.rs:71-80`）让工作区自带的 MCP 条目可以把任意进程环境变量放进请求头或参数发往任意 URL（工作区条目会进探测队列，为推断）。

**中**

- **S3 Webhook**：没有 `signing_secret` 时，128 比特的 URL 令牌是唯一的认证，本机任何进程都能触发智能体运行；HMAC 只覆盖负载、不覆盖 `X-Multara-Delivery`，抓到的已签名请求可以换一个新的投递 id 重放（去重键由攻击者选择，没有时间戳）；签名无效的请求每个插入一条 `rejected` 行（`INSERT OR IGNORE`，键由攻击者选择），数据库无限增长，从不清理；**不可信的负载原文被直接拼进智能体提示词**（`"{prompt}\n\n触发载荷：\n{payload}"`，`commands.rs:4994`）用于无人值守的运行，等于对带工具的智能体做提示词注入；`signing_secret` 明文存数据库。好的一面：只绑定 `127.0.0.1`、256KiB 上限、`verify_slice` 是常量时间、必需的自定义头挡住了浏览器 CSRF（OPTIONS → 405）。
- **S4 密钥落盘**：`mcp_oauth.rs:326-333` 令牌文件 `File::create` 之后才 `set_permissions`，同样有窗口、非原子；令牌明文存盘（只有 Unix 有 0600，Windows 没有 ACL）；`channel/secret.rs:34-50` 的 macOS 应用密钥以 `security … -w <value>` 放在命令行参数里，本机用户 `ps` 可见。
- **S5 `resources.rs` 的 `atomic_write` 权限降级**：见 10.1 第 1 条。
- **S6 LSP 会执行仓库自带的程序 [排查所得]**：`lsp.rs:168-177` 的 `resolve_program` 直接执行 `<工作区>/node_modules/.bin/<cmd>`，没有信任提示，打开不可信仓库并启动 LSP 就是执行仓库提供的代码。

**低**

- `mcp_oauth.rs`：服务器提供的元数据可以让它请求任意 https 主机（只允许 https、`--max-redirs 0`）；`Sec-WebSocket-Accept` 校验缺失（`accept_key` 是死代码）；`pkce_verifier` 是 uuid 字节重复两次（122 比特，够用）；`sign_out` 只删本地令牌，不撤销。
- 频道：`format_outbound` 把 `sender_id` 不转义地拼进 `<at user_id="…">`；卡片动作检查了 owner 角色但没绑定到发起的聊天，重放 `answer_question` 点击每次都会发一条新消息；`mcp_vars::read_env_file` 读取配置里指定的任意路径（`envFile`）。
- `webhook.rs:87-98`：错误到 JSON 的转义只处理 `\` 和 `"`，错误里有换行会让 JSON 失效；状态码靠 `error.contains("token")` 判断。
- `mcp_oauth.rs:548-556` `form_decode` 的 `&value[index+1..index+3]` 在非字符边界切片会 **panic**（比如回调 `?code=%é`，**本机任何进程访问回调端口都可触发**）[已核对]；`wait_for_code` 只接受第一个请求，所以对环回端口的一次杂散探测就会让整个授权流程失败。

### 10.3 协议驱动（`protocol.rs`）：会让运行提前结束、错误丢失、权限放行

1. **回合被 id 冲突提前终止（中高）[已核对]**：`protocol.rs:138-150` 的提示循环对**任何** `id == 3` 的消息都 `break`，不检查有没有 `method`。智能体主动发来的请求（比如 `session/request_permission`）id 恰好是 3 时，我们回应之后就把本轮结束了，驱动提前返回，运行以部分输出结束；`call()` 是对的（`234-237` 先检查 `method`）。同时，`id` 被回显成字符串时永远匹配不上，循环只能等 EOF。
2. **错误被静默吞掉（中）**：`call()` 收到错误响应返回 `None`（`241-243`）而不发出事件，所以 `initialize` / `session/new` 失败没有任何错误事件；`session/prompt` 的错误响应（`id==3` 且带 `error`）直接 `break`（`:145`），鉴权失败、提示过长看起来就像空的成功；`stopReason`（cancelled、refusal、max_tokens）被忽略。
3. **权限“失败即放行”（中）**：`decide`（`:64`）是 `recv_timeout(300s).unwrap_or(fallback)`，而 `fallback` 是**允许**选项（`301-317`），所以超时或发送端被丢弃就自动批准；`executor.rs:526` 的 `permission_prompt` 在数据库锁失败时也返回 `false`，等于全部自动批准；超时后 `pending` 条目不清理。
4. **跨运行的授权错配（中）[已核对]**：`executor.rs:577-596` 的 `resolve_permission` 在**所有** `active_runs` 里按 `request_id` 查找。ACP 智能体的 id 从小整数开始，两个并发运行都有 `"1"` 时，批准可能落到另一个运行的请求上。
5. **无界内存与额外分配（中）**：`mpsc::channel::<Vec<u8>>()`（`executor.rs:1123`）无界，`ByteSender::write` 里 `buf.to_vec()`，每个事件 `writeln!` 约 2 次发送；读线程卡在数据库上时队列无限增长、智能体永远得不到背压；`emit` 把 `json!` 序列化成字符串再写字节，`ByteReceiver` 再解析一遍。
6. **没有超时（低~中）**：`initialize` / `session/new` 的 `call()` 在 `read_line` 上永远阻塞；等权限时读线程也不读；`read_line` 没有长度上限，非法 UTF-8 当作 EOF；只能靠 kill 停止。
7. **`tool_call_update` 映射（低）**：`protocol.rs:365-395` 任何非空 `output` 且 `in_progress` 都会提前、反复地发出一个 “ok” 的 `tool_result`，这会**重置我加的 `consecutive_tool_errors`**；只读 `/content/0`，`diff` 内容被丢弃；`tool_call` 用 `name` / `title` 而忽略 `kind`。

### 10.4 LSP（`lsp.rs` 全读）

- **`lsp_send` 是同步命令，在主线程做阻塞写（高）[已核对]**：`lsp.rs:97-104`，`write_message` 对子进程 stdin 做阻塞 `write_all`，服务器慢、64KB 管道写满时（索引期间的大 `didOpen` / `didChange`）UI 冻结。应异步化或用有界队列的写线程。
- **stderr 读到第一个非法 UTF-8 行就停（中）**：`lsp.rs:154-161` 的 `reader.lines()` 出错就 `break`，之后 stderr 永不再被排空，服务器写满管道后阻塞，LSP 挂住；每行分配无上限。应改 `io::copy(&mut stderr, &mut io::sink())`。
- **`read_loop` 从不 kill 子进程（中）**：`lsp.rs:192-202`，任何 `read_message` 失败（头里非法 UTF-8、`read_exact` 不够、垃圾头）都被当成 EOF，然后把服务器从 `SERVERS` 移除并对一个**仍然活着**的进程 `child.wait()`：读线程永远阻塞，进程成孤儿且 `lsp_stop` 找不到它（已不在映射里），`wait()` 还持有 `child` 互斥锁，并发 `lsp_stop` 也会被卡住。应先 `kill()` 再 `wait()`。
- 每条 LSP 消息都以“JSON 套 JSON 字符串”的形式 `app.emit` 并广播给所有窗口（`:195`），不合并，`$/progress` 和诊断突发会淹没 IPC；`lsp_start` 是异步但在运行时工作线程上调用 `workspace_root`（`lock_db`，带 25ms 睡眠轮询）。
- 前面审计者的几条确认无误：`vec![0; length]` 无上限（`:225`）、`lsp_stop` 僵尸、启动竞态（`116` 的键检查与 `151` 的插入不是原子的）；后端没有请求 id 映射，唯一的映射 `SERVERS` 在退出时移除，没有增长问题。

### 10.5 启动、单实例、日志、锁

- **启动期全部同步在主线程（中）[已核对]**：`lib.rs:142-158`，数据库打开、迁移、`seed`（里面是已知的登录 shell `detect()`）、`ensure_shim` 都在 `setup` 里，而 `reveal_main_window` 已经先 `show()` 了窗口，用户看到的是空白、卡住的窗口；`database::open` 失败时 `?` 直接终止 `setup`，没有弹窗。还包括 `cleanup_old_snapshots`、`backfill_results`（`ops/automation.rs:104-140`：`LIKE 'automation-run:%'` 加 `'automation-run:' || r.id` 的连接，用不上索引，再加 `NOT EXISTS` 探测）、`recover_interrupted_runs`，启动耗时随历史增长。
- **没有单实例插件（中）[已核对]**：`lib.rs:127-136` 的插件只有 updater、process、dialog、window_state。第二个实例会打开同一个 SQLite 文件并各自运行轮询、自动化、webhook 和渠道 supervisor，**运行被重复派发、端口冲突**。
- 如果 `app_data_dir()` 失败（`lib.rs:145`），`McpToolsCache` 就不会被 manage，之后任何 `state::<McpToolsCache>()` 都会 panic。
- **`diag.rs` 的开销（中）**：一个 span 是 2 次 `write`（ENTER 和 LEAVE），`commands.rs` 里有 108 个 span；每次写入做 chrono `Local::now()` 格式化、`ThreadId` 的 `Debug` 格式化加 2 次 `replace`、全局互斥锁、对未缓冲 `File` 的 `writeln!`，约每行 2 个 `write(2)`，每个命令约 4 个系统调用，且在一把全局锁里做 IO；慢盘或 Windows 杀毒软件下每个命令（有些在主线程）都会被拖慢。我没有测量，估计每次写入是微秒级。
- **`lock_mutex` 的 25ms 睡眠轮询（中）[已核对]**：争用的获取要付 25ms 的整数倍，没有公平性，流式线程可能饿死 UI 命令；而且一次在持有数据库锁时的 panic 会让互斥锁中毒，之后**每次** `lock_db` 都返回 `Err` 直到重启。
- 没有 `PRAGMA synchronous=NORMAL`：WAL 加默认 FULL 在每次提交时 fsync，而流式事件插入都在这一把锁下（上一轮已列，这里再次确认没有设置）。

### 10.6 其它新发现

**高**

- **活动快照加载是 O(n²)（高）[形状已核对，量级为推断]**：`snapshot_db.rs:310-368` 的 `candidates` CTE 带着压缩后的 `content` BLOB，又被相关子查询 `id = (SELECT id FROM candidates later WHERE …)` 重新扫描，没有索引，对所有未处理快照是 O(n²)，还要把 BLOB 拷进临时表；之后对每行 zstd 解码，一次 IPC 全部返回（`load_review_baselines` 同样无上限）。它在启动时也会跑：`database.rs:691` 的 `backfill_file_review_baselines` 在 `file_review_baselines` 为空时执行。

**中**

- `commands.rs` 的 `list_channel_installations`（同步、持锁、每个安装一个 `security` 子进程）、`save_channel_installation`（持锁跨 `secret::store` / `load`）、`test_channel_installation`（主线程做网络 `bot_info`）：钥匙串解锁提示或慢钥匙串会冻结整个应用（§9.2 第 14 条已列，这里确认并补上 `save_` 路径）。
- **`webhook.rs:30-32, 53-62` 单线程 `incoming_requests` 循环，请求体内联读取且无超时 [已核对]**：一个客户端发完头不发体就会卡住所有 webhook，而且持数据库锁的派发也内联在这个循环里。
- **渠道 supervisor / 飞书 [排查所得]**：
  - `feishu::run` 没有 `catch_unwind`，注册表项保留，panic 杀掉线程后在指纹不变的情况下永远不会重启；`backoff` 连接成功后也不重置，会一直涨到 60 秒；
  - 每个入站事件一个线程，无上限（`feishu/mod.rs:158-163`），`handle_message` 里做数据库、网络和媒体下载（10 个文件 × 25MB，串行），陌生人或已配对用户的洪水会生成无上限线程；
  - 发了 ping 但从不检查 pong 或不活动，也没设 TCP keepalive，睡眠 / NAT 断开后套接字半开，机器人显示“已连接”但实际聋了，直到一次写失败；`connect_timeout` 只试第一个解析出的地址（IPv6 优先失败不会回退）；握手读取丢弃 `\r\n\r\n` 之后的字节 [推断]；
  - `outbound.rs:38-88` 的 `deliver` 没有原子认领，`on_run_finished`、`recover_pending` 和并发运行可以对同一会话同时 `deliver`，**重复回复**；内容是“会话里最后一条助手消息”，而不是按运行；
  - `claim_inbound` 对每条入站消息（包括未配对陌生人和没点名的群聊杂音）插入去重行，7 天清理只在（重）连接时跑，连接期间无限增长；
  - **未配对用户的每条消息都触发一次 `send_notice` API 调用加一条出站记录，没有限速**（`router.rs:70-80`），是对机器人 API 配额的放大 / 滥发手段。
- **`mcp_oauth.rs`**：`http_exchange`（`:384-434`）`curl` 把响应写临时文件、没有 `--max-filesize`，进程退出后才整体读入并检查 1MB 上限；`authorize` 里最多 6 次串行 20 秒的 `curl`；`access_token()` → `refresh()` 在 `mcp_vars::prepare`（运行 / 探测派发时调用）里做同步 20 秒 `curl`（调用线程未验证）；`refresh` 用普通 `fs::write` 且没有锁，探测 worker 和运行派发并发刷新时，轮换刷新令牌会让其中一个得到 `invalid_grant`，失败后回退用已过期的令牌（`:47`）。
- **`cursor_mcp_state.rs:52-80` [开启方式已核对，争用为推断]**：从同步命令（`set_mcp_enabled`、`save_mcp`）在主线程以读写方式打开 Cursor 正在使用的 `state.vscdb`，Cursor 持锁时会阻塞至 rusqlite 默认的 5 秒忙超时；JSON 列表的读-改-写会被 Cursor 刷新内存状态时覆盖；每次扫描（`disabled_global_ids_at`）还会打开一次。

**低**

- `mcp_tools_cache.rs:156-203` 的 `worker_loop` 里一次探测 panic 会让 `running=true` 永远不清，缓存此后再也不探测（推断）。
- `snapshot_db.rs:454-471` `get_turn_snapshots` 每个后续消息 id 一次查询（N+1）；`diff_state_db.rs:51` `load_all_chunk_states` 返回所有行无上限。
- 渠道媒体下载到 `channel-media/<inst>/<msg>` 从不清理（磁盘增长）；`outbound.rs:226` `split_message` 每行重新算 `chars().count()`，O(n²)；**单行超过 18000 字符永不被拆分**，API 拒绝后该投递被标为失败且不重试。
- `usage.rs:114-141, 185-195`：`extract_usage` 优先用累计的 `payload.info.total_token_usage` 而不是 `last_token_usage`，并归为 Step，没有结果事件时逐事件累加会让 codex 用量**重复计数**（推断）；未知模型价格静默记 0。
- `feishu/mod.rs:174-211` `reassemble` 的分片表对收不全的消息永不淘汰，`vec![None; sum]` 的 `sum` 取自服务端帧头。

### 10.7 提示词怎么传给 CLI（回答 §9.1 里留下的未验证项）

- **走 stdin（原始字节）**：cursor、qwen、opencode、codearts、pi、omp。
- **走 stdin（stream-json 信封）**：claude、codebuddy。
- **走 stdin 的 JSON-RPC**（有协议驱动时 `prompt_via_stdin` 强制为 true）：hermes、kimi、reasonix、dim、qwenpaw、mcode、zeroclaw、kiro、qoder、qoderclicn、traecli、grok（均为 ACP）和 dsh。这是从 `build_args` 里的 `acp` / `--stdio` 推断的，`protocol_driver` 没读。
- **走命令行参数（argv）**：codex（最后一个参数）、gemini（`-p X`）、copilot（`-p X`）、deveco（最后一个参数）、openclaw（`--message X`）、antigravity（`-p X`），以及默认分支 `_`（含未知 / 自定义提供商，`vec![assignment]`）。
- 命令行里放的是**完整的拼装后提示词**（`executor.rs:1024` `replace_assignment(launch_args, &run.assignment, &prompt)`），不只是用户任务，所以简介、历史、附件都在 argv 里，本机 `ps` 可见。`runtime_brief::build`（163 行，读完）只是指令模板，增加约 4~6KB，不做 IO。
- **E2BIG（300~500KB 提示词）**：macOS 没有单参数上限，只有 argv 加 envp 总共约 1MB（`ARG_MAX` 1,048,576），一个 300~500KB 的参数放得下，加上环境变量和中文每字 3 字节，约 900KB 以上才会失败；**Linux** 单个参数上限 128KB（`MAX_ARG_STRLEN`），会失败；**Windows** 整条命令行约 32,767 个 UTF-16 单元（`ERROR_FILENAME_EXCED_RANGE`，os error 206），所有走 argv 的提供商 300KB 都会失败，npm 的 `.cmd` 垫片（codex、gemini、copilot）还要经过 `cmd.exe`，上限 8,191 字符。建议对这些提供商改用临时文件或 stdin，或者截断。
- `replace_assignment`（`executor.rs:2431`）会替换**所有**等于任务文本的参数：用户消息恰好是 “run”、“exec”、“-p” 这类字符串时会破坏参数列表；`1017` 处用的是首个匹配的 `position`，两处不一致。
- `probe_cli_version`（`runtime.rs:701-720`）超时 8 秒后线程和 `--version` 子进程都没被杀，挂住的 CLI 会泄漏一个线程加一个进程，且 `None` 被永久缓存（`version_cache`），一次超时的探测要重启才会重试；`execution_for` 对自定义运行时每次调用都 `lock_db` 加 `detect_custom`（`which`，未命中再起登录 shell），在 `845`、`906` 和经 `custom_runtime_launch` 的 `provider_for` 里都会调用，轮询每 2 秒也会走；负路径查找没有缓存。

### 10.8 现在的覆盖情况

- **读完**：`lsp.rs`、`protocol.rs`、`lib.rs`、`diag.rs`、`process.rs`、`win_shutdown.rs`、`channel/mod.rs`、`builtin_agents.rs`、`mcp_vars.rs`、`runtime_brief.rs`、`resources.rs` 1024-1712、`mcp_oauth.rs`、`cursor_mcp_state.rs`、`usage.rs`、`webhook.rs`、`automation.rs`、`ops/automation.rs`、`session.rs`、`snapshot.rs`、`snapshot_db.rs`、`diff_state_db.rs`、`channel/supervisor.rs`、`channel/outbound.rs`、`channel/router.rs`、`channel/commands.rs` 1-324、`channel/feishu/mod.rs`、`channel/feishu/ws.rs`、`channel/secret.rs`。
- **读了一部分**：`executor.rs` 的 395-430、577-596、705-780、835-860、900-915、1000-1060、1115-1200、1330-1670、2425-2680、3405-3535 以及 67-107、524-537、1820-1840、2125-2160；`runtime.rs` 150-300、400-760；`database.rs` 25-125 与 575-905（略读）；`conversation.rs` 21-33、242-259；`git_merge.rs` 500-696 是纯测试模块（生产代码没有）；`code_index.rs` 230-475（辅助函数和测试，没有值得记的）；`channel/feishu/decode.rs` 225-424、`pb.rs` 195-322；`channel/store.rs` 237-253；`feishu/api.rs` 189-234；`commands.rs` 的 2225-2290、2340-2520、2690-2730、4040-4090、4840-5240；`mcp_tools_cache.rs` 120-215。
- **仍然没读**：`runtime.rs` 其余部分、`executor.rs` 其余部分、`protocol_driver` 和 `normalize_*`、`channel/store.rs`（除上述范围）、`channel/feishu/api.rs`（除 `download_resource`）、`channel/feishu/card.rs`、`database.rs` 125-575 与 905-1226、`merge_flow.rs` 没再读、`models.rs`（只 grep）、以及整个前端里没被前几轮审计覆盖的组件。
- 仍然**没有在运行中复现过**任何一条；`multara.log` 里的耗时记录依旧是唯一的实测数据。

### 10.9 建议的处理顺序（在 §6、§9.5 的基础上补充）

**立刻（数据 / 安全 / 正确性，改动都不大）**
1. `files.rs` 的 `resolve_existing` 加根目录包含检查，删除符号链接时只删链接（§9.0）。
2. `resources.rs::atomic_write` 改成真正原子：同目录临时文件 + `fsync` + 保留原权限 + 一次 `rename`；整文件读-改-写前后比较 mtime，变了就重读重合并。
3. `mcp_oauth::form_decode` 按字节处理百分号序列，不要在字符串上切片（会 panic）。
4. 配对码：加长（至少 8 位字母数字）、一次性、过期、尝试次数限制与锁定；指纹里加入 `pairing_code` 与 `group_enabled`。
5. `protocol.rs` 的提示循环只在“没有 `method` 的响应”时才 `break`；`call()` 和 `session/prompt` 的错误要发出事件；权限等待超时的回退改为**拒绝**。
6. `git_merge` 所有解析文件名的 git 输出加 `-z`；读不到文件时按“有冲突标记”处理。
7. Webhook：即使 `already_running` 也不要回 202 `dispatched`；投递行在派发成功后再标 `queued`；`execute_automation_source` 失败时把已插入的 `pending` 运行置为失败；负载包进明确的“不可信数据”分隔符，不要直接拼提示词。
8. 加 `tauri_plugin_single_instance`。

**随后（性能 / 稳定性）**
9. `PendingStream`：stderr 不起 flusher；Drop guard 保证 `stream_done`；熔断路径补 `persist_run_usage`；把合并改成 `String` 构建器，只在 flush 时生成条目，text / thinking 不做 `serde_json::from_str`，并且 flush 时只发增量而不是整条。
10. `lsp_send` 异步化；LSP stderr 用 `io::copy(sink)`；`read_loop` 先 kill 再 wait。
11. `diag`：去掉每行 `flush`（用 `BufWriter` 加定时 flush），运行期间也轮转；`lock_mutex` 改成真正的阻塞 `lock()`（中毒时恢复而不是永久 `Err`），保留超过 2 秒的告警。
12. 把 §9.2 与 §10.6 里的同步重命令统一改成 `async` + `spawn_blocking`，并且不要在持数据库锁时做 IO。

---

## 11. 第四轮补充：剩余后端文件（含 `commands.rs` 全量、`database.rs` 全量、`models.rs`）

标记同前。这一轮我自己抽查核对过的有：`run_events` 的 CHECK 约束（**连同本机真实数据库里的表定义**）、`truncate_and_redact` 的实现、`clear_runtime_transcripts` 的路径拼接、`open_external_url` 的 Windows 分支。

**顺便说一件现场情况**：我现在查本机数据库，`agent_runs` 里已经没有 `running` / `queued` 状态的运行了，之前那个失控运行（事件数一路涨到 2 万多条）已经结束。它留下的 `run_events` 现在是 `tool_use` 22,853、`tool_result` 22,833、`thinking` 1,073、`text` 391、`error` 24，其中大部分是那些空工具残片和报错，我上一轮加的“加载时过滤”会把它们挡在界面之外，但**这些行仍然在库里**，没有被删除。

### 11.1 数据正确性问题（会让功能悄悄坏掉）

1. **`plan` 和 `permission` 事件永远写不进数据库（中）[已核对：约束定义与本机真实表都是这样]**：`database.rs:187-192` 建表时 `event_type` 带 `CHECK(... IN ('text','thinking','tool_use','tool_result','error'))`，**我用 `sqlite_master` 看了本机数据库里的真实表，约束就是这五个值**；但 `executor.rs:3196` / `3207` 的 `normalize_wire` 会为 ACP / dsh 运行时发出 `plan` 和 `permission`，而 `persist_timeline_item`（`executor.rs:3494`）用 `let _ =` 吞掉了错误，所以这些插入全部静默失败。结果：**计划卡片和权限卡片在重新加载后消失**，尽管 `load_session_timelines` 是支持读取它们的。本机库里也确实没有任何 `plan` / `permission` 行（也可能只是没有跑过这类运行时，不能单凭这点断定）。
2. **`truncate_and_redact` 破坏正文，又漏掉真正的密钥（中）[已核对]**：`executor.rs:3359-3388`：
   - 用 `lines()` 再 `join("\n")`，会**丢掉末尾换行**，流式增量（`"hello\n"`、`"\n\n"`）失去段落间距，累积的 `output`（`1493-1498`）对只发增量的运行时被破坏；非 JSON 的普通 stdout 行（`executor.rs:2647-2651`）被直接拼接，**行与行粘在一起**。
   - 任何包含 `token=`、`password=`、`api_key=`、`apikey=`、`authorization:` 的行整行被替换为“[已隐藏敏感信息]”，**会把合法的代码和说明文字一起抹掉**（比如讲解环境变量配置的文字）。
   - 同时它**漏掉**：JSON 形式的 `"api_key": "sk-..."`、`"password": "x"`、裸的 `Bearer xxx`、`sk-...` / `ghp_...` 密钥、`AWS_SECRET_ACCESS_KEY=`、`SECRET=`、`client_secret=`、PEM 私钥块，以及被流式分块切开的密钥（每个增量独立处理）。
   - `sanitize_value`（`executor.rs:3390`）只按键名处理，所以工具 `input.command` 里内联的密钥不会被脱敏；它还会把任何含 “token” 的键（`max_tokens`、`token_count`、`tokenizer`）在工具参数里一并遮掉。
   - 工具 `input`（包括带内联密钥的 shell 命令）**以明文持久化到 `run_events.input_json`**；`finish()` 的错误文本和 `failure_reason` 用的是同一个弱脱敏函数；原始行还会进入 `conversation::persist_from_line`（已知）。
3. **表重建迁移没有事务，且会关掉外键（中）**：`database.rs:778-802`、`858-881` 在一次 `execute_batch` 里做 `PRAGMA foreign_keys=OFF; CREATE…; INSERT…; DROP TABLE; RENAME; PRAGMA ON`，中途失败可能丢表（`workspaces`、`automations`），并让这条连接的外键保持关闭。
4. **外键与级联缺失（低~中）**：`agent_runs.agent_id` 和 `squads.leader_agent_id` 没有 `ON DELETE`，所以 `delete_agent` 对任何运行过的智能体都会失败（`ops/agent.rs` 的错误文本自己也承认）。`delete_project`（`commands.rs:1097-1152`）里的 8 条删除不在事务里，并会留下孤儿 `tasks`（对 `chat_sessions` 没有外键）以及孤儿 `turn_file_snapshots` / `diff_chunk_states` / `file_review_baselines`；`delete_task`（`commands.rs:3181-3268`）停掉运行后立刻删行，**读流线程仍在为已删除的会话插入 `run_events`**（无外键，成为孤儿），而且从不删除 `turn_file_snapshots`（整文件 BLOB）、`diff_chunk_states`、渠道绑定和 `task-workspaces/<session>` 草稿目录；对 `messages`、`agent_runs`、`message_attachments` 的 `DELETE … WHERE session_id` 都是全表扫描。
5. **自动化任务补跑与父任务环（低）[推断]**：`advance_trigger`（`commands.rs:5152-5207`、`4898-5056`）按 `planned_at` 而不是“现在”推算下一次，睡眠或关机之后每分钟一次的 cron 会把所有错过的时间点各补一遍，每个产生一条 skipped 的 `automation_runs`；每个完成的运行还会永久地创建会话、任务和收件箱项。`ops/task.rs` 的 `maybe_resume_parent` 和 `update_meta` 只拒绝“自己是自己的父任务”，**A→B→A 的 `parent_task_id` 环会让每次完成都为对方排一个恢复运行，形成无限的智能体循环**（可通过 CLI 或 UI 触发）。
6. **`commands.rs:2344`**：`format!(r#"{{"event":"{}"}}"#, event_name)` 事件名里有引号会生成非法 JSON（JSON 注入）；`automation_runs`、`webhook_deliveries`、`channel_outbound_messages` 从不清理；`channel/store.rs::delete_installation` 不是事务。

### 11.2 其它卡顿 / 挂起（补充前面没写过的）

- **高 · `steer()` 持有全局 `active_runs` 锁时做阻塞的 stdin 写（`executor.rs:549-575`）[已在 §9.2 第 8 条记过，这一路独立确认]**：子进程不读 stdin（挂住，或 steer 消息超过 64KB）时，`stop()`（来自主线程的同步 `stop_run`）、`can_steer`、`resolve_permission` 和 2 秒一次的 `reap_orphaned_runs` 全部被卡住，**界面冻结，而且“停止”按钮也失灵**。
- **高 · `save_agent`、`configure_runtime`、`save_custom_runtime`、`delete_custom_runtime` 持数据库锁在主线程里跑 `detect()`**（已知，两路独立确认）。
- **中 · `generate_text`（`executor.rs:359-512`）**：提示词在读线程创建之前就写进 stdin（`445-452`），没有任何超时覆盖，CLI 不读 stdin 且提示词超过 64KB（压缩历史时）会**永远死锁**；`child.kill()` 只杀直接子进程，孙进程握着管道时 `stdout_reader.join()`（`482`）会挂住；`read_to_string` 无上限；同步调用者（标题 8 秒、压缩 90 秒）阻塞主线程。
- **中 · `runtime.rs:1017-1031` `login_path_dirs` 没有单飞**：并发冷启动的调用者各自起自己的 `-ilc` shell；`load_login_path_dirs`（`1052-1065`）三次尝试各 10 秒，最长 30 秒；`probe_runtime_command`（`commands.rs:1668`）是同步命令，会在主线程撞上这个。
- **中 · CLI 每次调用都做完整迁移**：`cli.rs:478` 的 `open_db()` → `database::open_file` → `migrate()` + `seed()`，约 100 次 `PRAGMA table_info`、16 次 `DROP TABLE IF EXISTS`、`UPDATE custom_runtimes`、`cleanup_old_snapshots`、`backfill_results`，以及基线为空时加载全部快照 BLOB 的 `backfill_file_review_baselines`；CLI 进程还会和应用的连接争写锁，应用正在流式写事件时容易出现 “database is locked”（5 秒忙超时）。
- **中 · 多处全表扫描 / N+1（均 [已核对]）**：`backfill_results`（`database.rs:~590` + `ops/automation.rs:104-115`）里 `LEFT JOIN automation_runs r ON s.id = 'automation-run:' || r.id` 是表达式连接，没有索引，复杂度 会话 × 运行；`ops/inbox.rs:5-21` 未读计数是对 `messages` 的逐项相关 `COUNT`；`truncate_messages_from`（`commands.rs:4173-4303`，同步、主线程、持锁）每条消息 4 次删除、每个回合还要 `DELETE FROM agent_runs WHERE turn_id=?`（无索引）和 `DELETE FROM agent_turns WHERE user_message_id=?`（无索引）；`list_sessions`（`commands.rs:4784-4817`）每个会话一次 `linked_project_ids`，而会话包括每个任务会话和每个 `automation-run:*` 会话，**从不清理**；`ops/task.rs::get()` 加载整个 `list()`（对每个任务解析标签和清单的 JSON）再 `find`，`update()` 调它 3~4 次，`ops/{project,squad,agent,automation}::get` 同样；`run_statistics`（`commands.rs:5238-5287`）里 `date(r.created_at,'localtime') >= …` 不能用索引，还对每个 `agent_runs` 行做相关 `EXISTS`，跑 4 遍，是同步命令；`stats()`（`:4834`）每次 bootstrap 都有 `date(created_at)` 全扫。
- **中 · `fork_session` 每条消息两次自动提交（`ops/chat.rs`）**：没有事务，WAL 默认 FULL 同步下每次提交一次 fsync，2000 条消息的分叉要 4000 次提交，在同步命令里持锁；而且 `database.rs:56-58` 没有设置 `synchronous=NORMAL`，`persist_timeline_item` 和 `persist_from_line` 也都是一次 fsync。
- **中 · 持锁做文件系统扫描的同步命令**：`save_mcp`、`set_mcp_enabled`、`delete_mcp`、`install_mcp`、`delete_skill`、`sign_out_mcp`、`list_resource_sources`、`convert_prompt_to_skill` 都调 `scan_all`（`commands.rs:2941-2950`），`resources::scan()` 在每次智能体运行的 `resource_context`（`executor.rs:2357`）里还会跑一遍，`builtin_skills::materialize` 每次运行重写 7 个 `SKILL.md`（`executor.rs:968`）。
- **低~中**：`list_chat_messages_blocking`（`commands.rs:3425-3528`）持锁返回整个会话的所有时间线（每个运行最多 `MAX_TIMELINE_ITEMS_PER_RUN` 条、输出最多 32k），`orphan_run_messages` 对每个没有消息的运行做 2 次查询；`save_pasted_attachment` 把最多 25MB 的 `Vec<u8>` 以 JSON 数字数组走同步 IPC，文件落到 `temp_dir()/multara-paste` 且从不清理；`check_git_executable` 同步运行用户提供的路径 `--version` 且无超时；每个 stdout 行最多被 JSON 解析 3 次（`persist_from_line`、`usage.ingest`、`normalize_runtime_line`），单个巨大的 JSON 行被整行持有并经 `sanitize_value` 克隆；每个 stderr 行都成为一条“error”时间线行（持久化加发出），吵闹的 CLI 会产生成千上万行。

### 11.3 安全（补充）

1. **会话 ID 拼路径后 `remove_dir_all`（中）[已核对代码路径；触发需要构造的 stdout 行]**：`conversation.rs:202-212`（由 `commands.rs:4218` 的 `clear_runtime_transcripts` 触达）：`id_from_line` 接受 CLI stdout 里任何顶层 `session_id` 字符串，**没有任何校验**；重新提问时 `transcripts.join(id)` 之后 `fs::remove_dir_all`，所以 `../../..` 或绝对路径（`Path::join` 遇到绝对路径会替换基路径）会删除任意用户目录。同一个 id 还会作为 `--resume <id>` 传给 CLI（以 `-` 开头就是参数注入）。
2. **Windows 上 `open_external_url` 命令注入（中）[已核对代码，cmd 行为为推断；这是前端那一路也发现的同一处]**：`commands.rs:3683-3706` 是 `cmd /C start "" <url>`，只检查 `http(s)://` 前缀，`&` 会被 `cmd.exe` 当作命令分隔符。URL 可以来自智能体输出和 Markdown。macOS / Linux 走 argv，是安全的。
3. **明文密钥回到网页层（中）[已核对]**：`get_custom_runtime`（`commands.rs:1589-1635` + `models.rs:243` 的 `CustomRuntimeForm.api_key`）把明文上游 API 密钥返回给 WebView，尽管已有 `has_api_key`；`settings_map`（`commands.rs:5462`）在 bootstrap 里返回**全部** `app_settings`，包括 `genApiKey` 和可能带凭据的 `updateProxy`；`custom_runtimes.api_key`、`automation_triggers.signing_secret`、`webhook_token` 都是明文存 SQLite。
4. **密钥出现在命令行参数里，本机任何用户 `ps` 可见（中）[已核对]**：`http_client.rs:62-69` 与 `executor.rs:306` 把 `Authorization: Bearer <key>` 用 `-H` 放进 curl 的 argv；`channel/secret.rs:27-50` 的 `security add-generic-password -w <value>`；非 macOS 的文件回退（`66-72`）先写后 `chmod`，有短暂的全局可读窗口；`runtime.rs:181` 把本地代理的 `ANTHROPIC_AUTH_TOKEN` 放在 `--settings` 参数里，本机用户可以复用这个 `127.0.0.1` 代理消耗上游额度。
5. **MCP 注册表包名直接进 `npx` / `uvx` 参数（中）[推断]**：`catalog.rs:158-164` 的 `packages[0].identifier`（第三方可控）直接作为 `["-y", identifier]`；形如 `--call=<cmd>` 或 `-c ...` 的标识符在探测或运行 MCP 时可以执行命令；远程 `url` 不检查协议。
6. **Webhook**：见 §10.2 S3（这一路独立确认：没有 `signing_secret` 就完全跳过验证，URL 令牌是唯一凭据，负载进入智能体提示词，无效签名的尝试按攻击者选定的 `dedupe_key` 插入 `webhook_deliveries` 行无限增长，没有时间戳 / 重放检查；HMAC 校验本身是常量时间，这点没问题）。
7. **`http_client.rs` 无 `--max-filesize`；`--location` 跟随重定向并带着自定义认证头**（curl 只剥离 `Authorization`，不剥离 `x-api-key`）（低~中）。
8. **子进程拿到数据库路径和全权限 CLI 垫片（低~中）[推断]**：`executor.rs:1066-1072` 给子进程 `MULTARA_DB`，智能体可以通过 CLI 读写整个 SQLite，包括明文 API 密钥；垫片是从整个应用可执行文件复制的（`install_cli_binary`），原地覆盖可能正在运行的二进制（macOS 上更新中途会让签名失效）。
9. **附件（低）[已核对]**：`send_message`（`commands.rs:3845`）允许附带任意本地文件（canonicalize 后、≤25MB）并把路径交给智能体；`load_attachment_preview`、`open_message_attachment`（`3623` / `3652`）读取或打开数据库里存的任意路径；对来自飞书的远程文件（`.terminal`、`.exe`）执行 `open`，用户点一下就可能执行。
10. **自定义运行时的 `env` 只剥离了受管键（低）**：`commands.rs:1433` 允许 `DYLD_INSERT_LIBRARIES`、`LD_PRELOAD`、`PATH`（只能由 UI 设置）。
11. **执行 / shell 站点清单**：`sh -c` / `cmd /C` 只出现在 `commands.rs:3692`（上面第 2 条）和登录 shell 的 PATH 探测（`runtime.rs:1053-1060`，`$SHELL -ilc` 加常量命令，不受攻击者影响）；其余都是不经 shell 的 argv（`process::command` / `Command::new`），没有注入。用户可影响的可执行文件：`check_git_executable`、`gitExecutable` 设置（`validate_settings` 不校验）、自定义运行时的 `command` / `args`、MCP 的 `command`（用户或注册表）、飞书附件文件名（`sanitize_file_name` 处理过）。命令处理函数里没有找到对用户输入的 `unwrap` / `expect`，命中的都在测试里。

### 11.4 会跨 IPC 的无上限载荷类型（`models.rs`）

- `BootstrapData`：所有项目、工作区、会话、任务、收件箱、自动化、资源，加上**整个** `settings` 映射（含 `genApiKey`）。
- `ChatMessage.timeline`（`input: Option<Value>`、每项 `output` 最多 32k、`content` 最多 64k）和 `source_meta: Value`。
- `RunEvent.snapshot_content`（每次文件修改发出完整文件文本）和 `RunEvent.timeline_item`，**应用级广播**。
- `FileContent.content`（媒体是 data URL）、`GitIndexFile.content`、`load_attachment_preview`（最多 10MB，base64 后约 13MB 字符串）、`SessionDiffData` / `ActiveDiffData`（全部快照和基线，已知）、`CustomRuntimeForm.api_key`、`McpInputs.values`（密钥）、`Automation.webhook_token`、`TaskInboxSurfaces`（全部任务、会话、收件箱）、`GatewayOptions.custom_headers`。

### 11.5 现在的覆盖情况

- **读完**：`runtime.rs` 1-150 与 760-1471（加上之前的 150-760，所以整个文件读完）；`executor.rs` 1-720、740-1335、1670-2430、2630-3412（加上之前的区间，基本读完，仅缺 720-740、1335-1425、1570-1670、2430-2630 里由早先审计者读过的区域与 3560-3744 的测试）；`channel/store.rs`、`channel/feishu/{card,api,decode,pb}.rs`；`database.rs` 1-1226 全部；`models.rs` 全部；`catalog.rs`、`builtin_skills.rs`；`hook_runtime.rs` 1-400；`ops/{mod,run,session}.rs`、`ops/project.rs` 1-95、`ops/skill.rs` 1-57、`ops/task.rs` 1-755、`ops/chat.rs` 1-215、`ops/agent.rs` 1-247、`ops/automation.rs` 1-209、`ops/inbox.rs` 1-171；`commands.rs` 960-5700（除 2545-2548、2977-3069、4320-4421 里的少量片段只略读）。
- **读了一部分**：`channel/secret.rs` 1-90、`http_client.rs` 40-230、`conversation.rs`（`id_from_line`、`clear_runtime_transcripts`）、`ops/squad.rs` 1-95、`cli.rs` 1-80 与 470-600 加 grep、`commands.rs` 4320-4421。
- **仍然没读**：`commands.rs` 1-960（异步的 git / 终端 / 搜索命令，前几轮审计者读过）和 5700-6281（测试）、`hook_runtime.rs` 400-514（测试）、`cli.rs` 80-470 与 600-799、`ops/squad.rs` 95-130、`executor.rs` 的测试模块；以及各文件里的 `#[cfg(test)]` 模块。
- 所有“慢”和“泄漏”仍是按代码推断的，**没有在运行中复现过**；本机数据库和 `multara.log` 依然是唯一的实测数据。

### 11.6 建议的处理顺序补充

**立刻（会让功能悄悄坏掉 / 会丢数据 / 有安全后果，每个都不大）**
1. `run_events` 的 CHECK 约束加上 `plan`、`permission`：SQLite 不能直接改约束，需要表重建迁移（要放进事务里，并且保持外键开启）；同时把 `persist_timeline_item` 里的 `let _ =` 改成至少记一条日志，不再静默吞错。
2. `truncate_and_redact`：不要用 `lines()` + `join` 丢换行；脱敏只作用于已知的密钥形态（`sk-…`、`ghp_…`、`Bearer …`、`"api_key": "…"`、PEM 块），而不是整行遮掉；对工具 `input` 也做同样的脱敏再落库。
3. `conversation::id_from_line` 校验 `session_id` 只含字母数字、`-`、`_` 且不以 `-` 开头；`clear_runtime_transcripts` 对拼出的路径 `canonicalize` 后确认仍在 `agent-transcripts` 下。
4. `open_external_url` 在 Windows 上改用 `ShellExecuteW`（或 `opener` crate），或至少拒绝 `& | ^ % " < >` 和空白。
5. `ops/task.rs` 的父任务检查改成沿 `parent_task_id` 链往上走，检测任何环。
6. `get_custom_runtime` 和 `settings_map` 不要把明文密钥发给前端，只发 `has_*`。

**随后**
7. 给 `database.rs` 的表重建迁移包事务，保持外键开启；补上 `agent_runs.agent_id`、`squads.leader_agent_id` 的删除行为；`delete_project` / `delete_task` 做成一个事务并清理快照、基线和草稿目录。
8. 把 `steer()` 的 stdin 写移出 `active_runs` 锁；`generate_text` 先启动读线程再写 stdin，并加超时和进程组；CLI 走轻量打开（不迁移、不 seed）。
9. 按 §6、§9.5、§10.9 的清单继续（索引、`synchronous=NORMAL`、同步命令异步化、`scan()` 缓存、日志轮转）。

---

## 12. 第五轮补充：`commands.rs` 前 960 行、`cli.rs`、`git.rs`、`explorer_fs.rs`、`terminal.rs`、执行器剩余区间

标记同前。我自己抽查核对过的有：`MULTARA_DB` 等环境变量传给子进程、`explorer_fs::entry_path` 没有根目录包含检查、`transfer_one` 只检查“目标在源里面”、终端快照 id 直接拼路径、`git_cmd` 没有覆盖 `core.fsmonitor`。

### 12.0 这一轮最重要的：智能体被给了一条通往整个数据库的路

**S1（高）[已核对环境变量；CLI 无权限检查为排查所得]**：`executor.rs:1062-1072` 给每个智能体子进程设置了 `MULTARA_DB`（指向 `multara.sqlite3`）、`MULTARA_APP_DATA`、`MULTARA_CLI`。

- 任何有 shell 的智能体都可以**直接打开 SQLite 文件**，里面有明文的 `custom_runtimes.api_key`、设置里的 `genApiKey`、渠道和 webhook 密钥。这绕开了网关的设计——网关只给子进程发一个代理令牌（`prepare_gateway`）。
- 它还可以**写**库：把 `gitExecutable` 设成攻击者的二进制，应用下次做 git 操作就会执行它；改智能体、启用自动化。
- `multara` CLI 与调用它的运行**没有绑定**：`MULTARA_AGENT_ID`、`MULTARA_SESSION_ID` 只是普通环境变量，`multara` 可以指向任意数据库（`cli.rs:497`）。通过 CLI，任何运行都可以：`task create --assignee <任意智能体>` 和 `session resume <任意 id> --prompt …`（在权限更高或 yolo 模式的智能体上派发运行，提权加提示词注入）、`agent update <id> --instructions/--runtime-id`（包括 Miku，持久化的提示词注入）、`automation create --enabled`（持久化）、`channel enable|disable`、`terminal read`（返回**未脱敏**的终端滚动内容，里面打印过的密钥都在）。没有找到任务创建的速率或深度限制（只看了 `ops::task::create` 的父任务处理，推断）。
- **S2（中）[已核对 `cli.rs:469-474`]**：`--description-file` 和 `task update` 读取任意路径（`fs::read_to_string`，无大小上限、无根目录检查），内容变成任务描述，可以通过 `task get` 读出；受限或带权限闸门的智能体可以把 `multara` 当作任意文件读取原语，例如 `~/.ssh/*`。
- 这是设计层面的问题，不是单点 bug：如果打算让智能体运行在“不可信”的权限模型下，就需要把 CLI 改成走带令牌认证的本地 IPC，并让令牌绑定到具体运行；至少不要把 `MULTARA_DB` 路径交给子进程，以及对 `terminal read`、`--description-file` 加限制。

### 12.1 数据丢失 / 正确性

1. **`transfer_one` 覆盖时可能删掉源（高）[已核对检查逻辑]**：`explorer_fs.rs:422-475`，`overwrite=true` 时先 `remove_path(&target)` 再复制或移动；“文件夹放进它自己里面”的检查只判断**目标父目录是否在源里面**（`440-462`），不判断**源是否在目标里面**。把 `a/b` 覆盖移动到 `a` 上，会先删掉 `a`（连同里面的源），然后移动失败，一切丢失，没有备份；删除目标之后复制失败也会留下空缺。应先复制 / 重命名到临时名再交换。
2. **`explorer_fs::entry_path` 的符号链接父目录逃逸（高）[已核对]**：`explorer_fs.rs:161-177` 只 canonicalize 父目录，没有 `starts_with(root)`；`link -> /etc` 加 `link/x` 解析到工作区之外。这和 §9.0 的 `files.rs` 是同一类问题，但**在另一个文件里**，修 `files.rs` 不会修到这里。可达路径：`explorer_delete_entries`（还会把目标复制进 `explorer-undo`）、`transfer`、`create_entry`、`write_bytes`、`explorer_open_external`、`explorer_absolute_path`、`explorer_clipboard_write_files`；`entry_path_creating_parents`（`:191`）的 `create_dir_all(root.join(parent))` 会跟随符号链接在根目录之外建目录；`write_bytes:495-503` 是 `exists_no_follow` 之后 `fs::write`，`overwrite=true` 时会穿过最终分量的符号链接写入，且不是原子的。
3. **`explorer_git_ignored` 管道死锁（中）[已核对]**：`explorer_fs.rs:654-659` 在同一个线程里先把所有路径写进 `check-ignore --stdin`，再读 stdout，没有超时。子进程的 stdout 写满（约 64KB 的被忽略路径，比如很大的 `node_modules` 列表）时，子进程阻塞在写上，父进程阻塞在 `write_all` 上，阻塞线程池的这个线程永远挂住。应该用单独的线程写 stdin。
4. **`git.rs` 重命名分支会删掉远程的同名分支（中）[排查所得]**：`git.rs:499-566` 带 `rename_remote` 时先推送新名，再 `push remote --delete <旧名>`，旧名取自上游，没有受保护分支检查；重命名本地 `main` 会删掉 `origin/main`；不是原子的（远程一步失败时本地已改名）；`upstream.split_once('/')` 对名字里带 `/` 的远程会误解析。
5. **`git.rs` 暂存和丢弃用的是 pathspec 而不是字面路径（中）[排查所得]**：`git.rs:125-176`，`validate_relative` 放行 `:(top)`、`:/`、`*`、`[x]`（在 Unix 上它们都是 `Component::Normal`），而 `git_cmd` 没有设 `GIT_LITERAL_PATHSPECS` 或 `--literal-pathspecs`；路径 `:/` 或 `*` 会让 `restore --worktree` / `clean -fd` 命中所有文件，名叫 `[id].tsx` 或 `a*b` 的文件也会匹配到兄弟文件。只有前端能提供这个值，所以是数据丢失的纵深防御；`discard` 还只对第一行 porcelain 做 `find_map`，对状态混杂的目录路径是错的。
6. **`cli.rs::parse_args` 会丢掉以 `-` 开头的参数值（中）[已核对]**：`cli.rs:625-645`，`--description "- item"`、`--title "-x"`、`--prompt "---"` 会被存成 `"true"`，真正的值落进 `positional`；以 `- ` 或 `---` 开头的 Markdown 描述会悄悄变成 `description="true"`，还可能让位置参数 `id` 和 `status` 错位；恰好等于 `-h` 的描述或评论会触发帮助而不是命令（`:81`）。智能体通过 CLI 写任务描述时很容易踩到。
7. **`cli.rs::create_automation` 忽略计划（低）[推断]**：`cli.rs:437-446`，`--schedule` 把 `trigger_type` 设成 `interval` 但 `interval_minutes` 为 `NULL` 且没有触发器行，`legacy_interval_cron(None)` 是错误（`commands.rs:5140`），多半不生效或显示错乱。
8. **`install_cli_binary` 原地覆盖正在运行的二进制（中）[推断]**：`cli.rs:567` 的 `fs::copy` 覆盖已有的 `multara`：macOS 上原地覆盖已执行过的已签名二进制（同一个 inode）可能让进程因签名缓存失效被 SIGKILL，应该先 unlink 再新建；不是原子的，覆盖期间并发 exec 会运行被截断的二进制；戳记在复制**之后**才写，崩溃会留下被截断的二进制直到下次启动；Windows 上旧 CLI 在运行时复制会静默失败，CLI 一直是旧的，戳记也从不更新，所以**每次启动运行时都重试**（`ensure_shim` 在 `executor.rs:1063` 每次运行都调用）；垫片比应用旧时，整个 GUI 可执行文件被当作 CLI 复制。
9. **`explorer_open_external` 用 `.spawn()` 不 `wait`（低）[已核对]**：`explorer_fs.rs:726-730`，每次“用外部程序打开”在 Unix 上都留下僵尸进程直到退出。

### 12.2 卡顿 / 挂起（补充）

- **执行器 H1（高）[已核对]**：`executor.rs:~1290-1305`，正常退出后的 `reader.join()` 路径**从不调用 `kill_child`**，孙进程握着管道时运行一直挂住，**进程组也不会被杀**（`kill_child` 只在停止、超时、stdin 出错时才跑）；而 `run_timeout` 默认 0（无限，`executor.rs:1413`），没有空闲超时。修法：`try_wait` 成功后对进程组 `kill -KILL -pgid`，或给读线程 `join` 加截止时间。（这条前面已出现，这一路独立确认，并补充了“进程组泄漏”这一面。）
- **`git.rs::run_with_timeout` 在钩子孙进程存在时无法兑现超时（中）[推断，管道的常规行为]**：`git.rs:1084-1112`，超时只杀 git，husky、`sleep` 之类的钩子脚本仍握着 stdout / stderr 管道，`join()` 一直等到钩子自己结束，120 / 180 秒超时失效，命令不返回，两个线程泄漏；另外这个 `spawn` 继承了 stdin（不同于 `.output()`）。`run` / `output`（`:1028-1062`）**完全没有超时**：`merge`、`switch`、`cherry-pick`、`status`、`blame` 都可能跑钩子、fsmonitor 或 GPG / 编辑器提示然后永远挂住，stdout 也无上限。
- **`git.rs` 的 `status()` 约 8 个串行子进程，还拿可选锁（中）[推断]**：`list_project_git_status`（`commands.rs:489-523`）对每个工作区串行执行；没有 `--no-optional-locks`，轮询会和用户提交撞出 `index.lock` 错误；`staged_diff:399-407`（`generate_git_commit_message`）先把整个暂存区 diff 读进内存再截到 24k 字符，大的 lockfile 或二进制 diff 会撑爆内存；`show_spec` 和 `blame` 同样缓冲整个 blob。
- **网络 git 缺 SSH 保护（中）[推断]**：`git.rs:1071-1075` 有 `GIT_TERMINAL_PROMPT=0`，但没有带 `BatchMode=yes` 的 `GIT_SSH_COMMAND`，也没有覆盖 `SSH_ASKPASS` / `GIT_ASKPASS`；没有 ssh-agent 的带口令密钥或主机密钥提示会一直挂到 180 秒超时（受上一条影响还可能更久）。
- **`terminal_write` 等同步终端命令（中）[已核对]**：见 §1.4，这一路独立确认；补充：`terminal_resize`、`terminal_close`、`terminal_publish` 也都是主线程同步命令，`publish_snapshot` 每次整份重写滚动内容 `.txt` 和 `index.json`，持有 `snapshot_io`。
- **同步文件命令（中）[已核对]**：`read_workspace_file`、`read_external_file`（`commands.rs:232-246`）最多 128MB，之后有损转 String、媒体 base64（32MB）、序列化给 WebView，内存约为文件大小的 3~4 倍；`explorer_stat`、`explorer_create_entry`、`explorer_open_external`、`explorer_absolute_path`、`explorer_clipboard_write_files` 是同步的，各自在主线程 `lock_db`；`explorer_clipboard_read_files` 在主线程同步跑 `osascript`（`explorer_fs.rs:816-820`），macOS 自动化权限弹窗会冻结界面；`lock_mutex` 没有超时，任何长时间持库的人都会冻住这些命令，而 `finish()` 在 `dispatch_squad_members` 期间也持有数据库锁。
- **低**：`terminal.rs` 的 `command_names()` 永久缓存 PATH 扫描，首次在主线程（同步命令）跑，之后一直过期到重启；`terminal` 快照只由 `terminal_close` 删除，崩溃后残留，`terminal list` 会出现幽灵会话；`close` 只丢弃 pty，忽略 SIGHUP 的进程会存活；`provider_for` / `execution_for`（`executor.rs:2477-2484`）每次调用都拿数据库锁，`detect_custom` 还会为 `cli_version` 起子进程，`dispatch_pending`（`:845`）和 `spawn_run`（`:906`）都会调；`custom_runtime_launch` 把所有数据库错误都映射成“自定义运行时不存在”（`:2632`）；`finish()` 在 `lock_db` 失败（中毒）时什么都不做，运行就一直是 running 且没有事件；`kill_child` 不检查子进程是否已被回收就 `kill -KILL -<pid>`（极小的 PID 复用窗口）。

### 12.3 安全（补充）

1. **终端快照 id 路径穿越（中）[已核对]**：`terminal.rs:712-754`，`terminal_publish` 的 `id` 来自 WebView，直接 `dir.join(format!("{id}.txt"))`（`:724`、`:742`），没有校验；`id="../../x"` 可以把攻击者选定的文本写到任何用户可写的 `*.txt`，`remove_snapshot` 也同样删除任意 `*.txt`；`index.json` 里的 id 只用来匹配，所以 `terminal read` 不受影响。
2. **`free_port` 会杀掉任何监听该端口的进程（中）[排查所得]**：`terminal.rs:879-896` 用的是系统范围的 `listen_ports()`，而不是终端会话的后代进程（不同于 `services()`），`kill -9` 发给任何同用户的匹配进程，比如 5432 上的 Postgres；不检查 kill 结果，`?` 会中途中止循环。
3. **`explorer_open_external` 执行工作区内容（中）[排查所得]**：`explorer_fs.rs:709-731` 对工作区文件调 `open` / `xdg-open`，而工作区内容由智能体和克隆下来的仓库控制，打开 `.command`、`.app`、`.jar`、`.desktop`、`.bat` 就是执行；配合 12.1 第 2 条还能打开工作区之外的路径。
4. **`transfer_one` 在没有 `source_workspace_id` 时接受任意绝对源路径（中）[排查所得]**：`explorer_fs.rs:433-440`，`copy=false` 时 `move_path` 会重命名或删除原文件，被攻破的 WebView 可以外泄或销毁用户拥有的任意文件；解引用的目录复制（`fs::metadata` 跟随符号链接）会把密钥拉进工作区；`files.rs:223,303` 的 `read_absolute` / `list_absolute`（`read_external_file`、`list_external_directory`）同样没有根目录限制。
5. **git 在不可信仓库配置下运行（中）[推断]**：没有任何 git 调用覆盖 `core.fsmonitor`、`core.sshCommand`、`core.hooksPath`、`protocol.*`（`git_cmd`，`git.rs:1017`，我读过，只设了 Windows 的 `creation_flags`）。打开或自动轮询（`list_project_git_status`、`explorer_git_remote`）一个不可信仓库，就可能通过 `status` 上的 fsmonitor 执行攻击者的命令；`fetch` 用的远程名取自 `git remote` 输出且没有 `--`，仓库里定义一个形如 `--upload-pack=…` 的远程名就可能注入。缓解：`-c core.fsmonitor=false`、`--no-optional-locks`、`--end-of-options`。
6. **`explorer_git_remote` 原样返回 `remote get-url origin`（低~中）[排查所得]**：`explorer_fs.rs:688-707`，URL 可能内嵌凭据（`https://user:token@…`），未脱敏就发给 WebView。
7. **`git tag` 的选项注入（中）[排查所得]**：`git.rs:475-480` 的标签动作：`check-ref-format refs/tags/{tag}` 接受前导 `-`（`-a`、`-e`、`-fX`、`-F/path`、`--delete`），因为 `refs/tags/` 前缀把它藏住了，而 `git tag <tag> <hash>` 没有 `--`；影响有限（可以删除与哈希同名的标签、强制把标签打到 HEAD，`-a` / `-e` 会拉起编辑器，GUI 编辑器会挂住），是 `git.rs` 里唯一没保护的用户可控选项位置；`set_branch_upstream:578` 是安全的（值被当作选项参数消费）。
8. **子进程继承完整的父环境（低）[排查所得]**：`executor.rs:1045-1088` 没有 `env_clear`，只加 `CI=1`，父环境里的 `ANTHROPIC_API_KEY` 之类都会透传，`managed_env_keys` 只对网关和自定义运行时剥离；`runtime::gateway_settings_args(..., proxy.token())` 把代理令牌放在命令行（`executor.rs:2541`），本机用户 `ps` 可见；终端 shell 集成用 `app_data` 里的 `ZDOTDIR` 垫片，**不会改 `~/.zshrc`**（这点是好的），fish 版把脚本路径插进单引号（`terminal.rs:518-526`），路径含 `'` 会出错；`explorer_write_file` 解码无上限的 base64（内存里两份），没有大小上限；`explorer_clipboard_write_files`（macOS）对 AppleScript 转义了 `\` 和 `"`，没发现注入。

### 12.4 git 参数注入的结论

- `validate_branch`：拒绝前导 `-` 和空白，并运行 `check-ref-format --branch`，是可靠的；`@{-1}` 只解析上一个分支名；没有 `--`，但前导 `-` 已被拒，可以接受。
- `validate_revision`：只允许 `HEAD` 或 4~64 位十六进制加可选 `^`，严格且安全，注入不了选项。
- `validate_relative`：拦绝对路径和 `..`，**不拦 pathspec 魔法**（见 12.1 第 5 条）；`stage`、`discard`、`blame`、`history` 里路径都放在 `--` 之后。
- 提交信息经 `-m` 传递，安全；push / fetch 里的 `remote` 和 `branch` 来自 git 输出（见 12.3 第 5 条）。
- 没保护的：标签名（12.3 第 7 条）和不带超时的 `merge` / `switch` 钩子（12.2）。

### 12.5 覆盖情况

- **读完**：`commands.rs` 1-960、2535-2554、2970-3074、4315-4424、4524-4570、5138-5155；`cli.rs` 1-799 全部；`git.rs` 1-1512 全部；`explorer_fs.rs` 125-849；`ops/squad.rs` 95-130、`ops/agent.rs` 247-403、`ops/automation.rs` 209-318、`ops/inbox.rs` 171-238（后四段都是 `#[cfg(test)]` 测试模块，没有生产代码）；`executor.rs` 395-440、715-805、985-1115（含环境构造、网关调用、spawn）、1236-1352、1570-1800、2425-2634。
- **读了一部分**：`terminal.rs` 60-222、483-546、565-800、851-960；`files.rs`（只看 grep 上下文：大小上限、`read_absolute`、`write_atomic`）；`executor.rs` 1115-1236（略读）。
- **没有读**：`hook_runtime.rs` 400-514 和 `ops/task.rs` 755-842（测试模块，没打开）；`ops/project.rs`、`ops/skill.rs` 本来就比指定起始行短，没有内容；`executor.rs` 的 1-395、440-715、805-985、1800-2425、2634-3520 是前面几轮读过的区间；`terminal.rs` 1-60、222-483（profiles 与集成脚本）、800-850、960-1162；`explorer_fs.rs` 1-125 与 849-1276（监听器代码与测试）；`gateway_proxy/`（没有确认代理是否校验令牌、绑定到哪个地址）；`commands.rs` 上述范围之外的部分。
- 全部结论仍是读代码得出，**没有在运行中复现过**。

### 12.6 建议的处理顺序补充

**立刻**
1. `explorer_fs::entry_path` / `entry_path_creating_parents` 和 `files.rs::resolve_existing` 一起修：统一一个“解析并校验仍在根目录内”的函数，所有入口都走它；删除符号链接只删链接。
2. `transfer_one`：覆盖时先复制到临时名再交换，并双向检查“源在目标里 / 目标在源里”。
3. `terminal_publish` / `remove_snapshot` 对 `id` 只接受 UUID 字符集。
4. `cli.rs::parse_args`：只有 `--` 开头的才当作标志，其它（包括以 `-` 开头的值）都当作值。
5. 设计层面：不要把 `MULTARA_DB` 交给子进程，CLI 走带运行令牌的本地 IPC，`terminal read` 与 `--description-file` 加限制。

**随后**
6. `git_cmd` 统一加 `-c core.fsmonitor=false -c core.hooksPath=/dev/null`（或按需保留钩子）、`--no-optional-locks`，并把所有 git 调用放进带进程组的超时封装里。
7. 执行器：正常退出后对进程组 `killpg`，并给读线程 `join` 加截止时间。
8. `explorer_git_ignored`：写 stdin 放到单独线程。
