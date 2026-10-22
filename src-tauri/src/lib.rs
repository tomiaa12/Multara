pub mod app_locale;
pub mod automation;
pub mod builtin_agents;
pub mod builtin_skills;
pub mod catalog;
pub mod cli;
pub mod cli_catalog;
pub mod code_index;
pub mod commands;
pub mod conversation;
pub mod cursor_mcp_state;
pub mod database;
pub mod diag;
pub mod diff_state_db;
pub mod executor;
pub mod explorer_fs;
pub mod files;
pub mod git;
pub mod git_merge;
pub mod http_client;
pub mod mcp_oauth;
pub mod mcp_probe;
pub mod mcp_tools_cache;
pub mod mcp_vars;
pub mod merge_flow;
pub mod hook_runtime;
pub mod lsp;
pub mod models;
pub mod ops;
pub mod process;
pub mod protocol;
pub mod resources;
pub mod runtime;
pub mod runtime_brief;
pub mod runtime_models;
pub mod session;
pub mod snapshot;
pub mod terminal;
pub mod text_search;
pub mod usage;
pub mod snapshot_db;
pub mod webhook;
pub mod workspace;

#[cfg(target_os = "macos")]
mod macos_menu;

#[cfg(windows)]
pub mod win_shutdown;

use commands::{
    add_workspace, authorize_mcp, bootstrap, check_git_executable, configure_runtime, create_chat_session,
    list_agents_and_squads,
    create_project, create_skill, create_task, create_workspace_directory, create_workspace_file, delete_agent, update_project,
    delete_automation, delete_custom_runtime, delete_file_baseline, delete_mcp, delete_project, delete_skill, delete_snapshots_from, delete_squad,
    set_project_archived,
    detach_workspace,
    delete_task, delete_workspace_entry, detect_runtimes, generate_chat_session_title,
    generate_branch_name, generate_git_commit_message, get_mcp, get_run_statistics, get_snapshot_content, get_turn_snapshots,
    git_abort_workspace_merge, git_commit_files, git_commit_multiple, git_commit_workspace,
    git_blame_line, git_discard_files, git_fetch_workspace, git_head_file, git_index_file,
    git_log_workspace,
    git_merge_workspace_branch, git_pull_workspace, git_push_workspace, git_rename_workspace_branch, git_set_branch_upstream, git_repair_worktree_runtime,
    git_graph_commit_action, git_revision_file, git_stage_files, git_stage_repository,
    git_switch_workspace_branch,
    git_sync_workspace, agent_system_context, get_custom_runtime, list_agent_runs, list_runtime_models, list_automation_runs, list_chat_messages, list_task_runs, mark_inbox_read,
    mark_inbox_unread, mark_inbox_all_read, archive_inbox_item, delete_inbox_item, fork_chat_session,
    set_message_feedback, compact_chat_session, resolve_permission, update_task_meta,
    convert_prompt_to_skill, generate_skill, import_skill_from_folder, import_skill_from_url,
    install_mcp, list_mcp_tool_cache, list_prompt_files, list_resource_sources, probe_mcp, reveal_resource,
    schedule_mcp_tools_refresh,
    search_catalog, set_skill_enabled, sign_out_mcp, save_mcp_inputs, get_mcp_inputs, get_mcp_json,
    list_external_directory, list_project_git_status, list_skill_directory, list_webhook_deliveries, list_workspace_directory,
    collect_source_files, collect_workspace_typings, list_quick_open_files, list_workspace_tree, load_active_diff_data, load_attachment_preview, load_session_diff_data,
    search_workspace, terminal_close, terminal_command_names, terminal_clipboard_text, terminal_free_port, terminal_open, terminal_profiles, terminal_resolve_paths, terminal_publish, terminal_resize, terminal_services, terminal_write,
    open_external_url, open_message_attachment, read_external_file, read_skill_file, read_workspace_file, receive_automation_webhook,
    rename_workspace_entry, reveal_workspace_path, run_automation, save_agent, save_automation,
    save_custom_runtime, probe_runtime_command, test_runtime_connection, save_file_baseline, save_mcp, save_pasted_attachment, save_settings, save_squad, scan_resources,
    set_mcp_enabled, skill_file_count,
    scan_workspace_candidates, send_message, stop_run, transfer_resource, trigger_automation_event,
    truncate_session_from_message, update_chat_session_target, update_chat_session_title, upsert_inbox_item,
    update_chunk_state, update_task, update_task_status, write_skill_file, write_workspace_file,
};
use tauri::Manager;
use tauri_plugin_window_state::{StateFlags, WindowExt};

const RESTORED_WINDOW_FLAGS: StateFlags = StateFlags::from_bits_truncate(
    StateFlags::SIZE.bits()
        | StateFlags::POSITION.bits()
        | StateFlags::MAXIMIZED.bits()
        | StateFlags::FULLSCREEN.bits(),
);

fn reveal_main_window(app: &tauri::App) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };

    #[cfg(windows)]
    {
        window.set_decorations(false)?;
        let _ = window.set_shadow(true);
    }

    // 只在这里恢复一次。插件 on_window_ready 若再 restore，会在 macOS
    // maximize()/zoom 动画中途再次 set_size + maximize，表现为白窗从左上角
    // 放大到一半又重来几次（新系统上更明显）。
    let _ = window.restore_state(RESTORED_WINDOW_FLAGS);
    window.show()?;
    let _ = window.set_focus();
    Ok(())
}

pub fn run() {
    #[cfg(windows)]
    win_shutdown::install();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(RESTORED_WINDOW_FLAGS)
                // 由 reveal_main_window 在 show 前统一恢复，避免与插件初始 restore 叠两次 zoom。
                .skip_initial_state("main")
                .build(),
        );

    #[cfg(target_os = "macos")]
    let builder = builder.menu(macos_menu::app_menu);

    builder
        .setup(|app| {
            reveal_main_window(app)?;
            let state = database::open(app.handle())?;
            if let Ok(data_dir) = app.handle().path().app_data_dir() {
                let _ = cli::ensure_shim(&data_dir);
                app.manage(mcp_tools_cache::McpToolsCache::load(
                    data_dir.join("mcp-tools-cache.json"),
                ));
            }
            app.manage(state);
            automation::start(app.handle().clone());
            webhook::start(app.handle().clone());
            executor::start_poller(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            list_agents_and_squads,
            create_project,
            update_project,
            create_task,
            delete_project,
            set_project_archived,
            merge_flow::git_merge_preview,
            merge_flow::git_merge_into_branch,
            merge_flow::git_finish_merge_session,
            merge_flow::git_pull_merge_session,
            merge_flow::git_abort_merge_session,
            detach_workspace,
            add_workspace,
            scan_workspace_candidates,
            list_workspace_tree,
            list_quick_open_files,
            collect_source_files,
            collect_workspace_typings,
            lsp::lsp_start,
            lsp::lsp_send,
            lsp::lsp_stop,
            search_workspace,
            terminal_open,
            terminal_profiles,
            terminal_free_port,
            terminal_resolve_paths,
            terminal_clipboard_text,
            terminal_command_names,
            terminal_write,
            terminal_resize,
            terminal_close,
            terminal_publish,
            terminal_services,
            list_workspace_directory,
            list_external_directory,
            read_workspace_file,
            read_external_file,
            write_workspace_file,
            create_workspace_file,
            create_workspace_directory,
            rename_workspace_entry,
            delete_workspace_entry,
            reveal_workspace_path,
            explorer_fs::explorer_list_directory,
            explorer_fs::explorer_stat,
            explorer_fs::explorer_delete_entries,
            explorer_fs::explorer_restore_entry,
            explorer_fs::explorer_transfer_entries,
            explorer_fs::explorer_create_entry,
            explorer_fs::explorer_write_file,
            explorer_fs::explorer_git_ignored,
            explorer_fs::explorer_git_remote,
            explorer_fs::explorer_open_external,
            explorer_fs::explorer_absolute_path,
            explorer_fs::explorer_clipboard_write_files,
            explorer_fs::explorer_clipboard_read_files,
            explorer_fs::explorer_watch_workspaces,
            list_project_git_status,
            git_stage_files,
            git_discard_files,
            git_stage_repository,
            generate_branch_name,
            generate_git_commit_message,
            git_commit_workspace,
            git_commit_multiple,
            git_graph_commit_action,
            git_switch_workspace_branch,
            git_rename_workspace_branch,
            git_set_branch_upstream,
            git_merge_workspace_branch,
            git_fetch_workspace,
            git_index_file,
            git_head_file,
            git_blame_line,
            git_revision_file,
            git_log_workspace,
            git_commit_files,
            git_pull_workspace,
            git_push_workspace,
            git_repair_worktree_runtime,
            git_sync_workspace,
            git_abort_workspace_merge,
            detect_runtimes,
            configure_runtime,
            save_custom_runtime,
            get_custom_runtime,
            delete_custom_runtime,
            probe_runtime_command,
            test_runtime_connection,
            scan_resources,
            create_skill,
            generate_skill,
            import_skill_from_folder,
            import_skill_from_url,
            delete_skill,
            set_skill_enabled,
            list_prompt_files,
            convert_prompt_to_skill,
            list_resource_sources,
            search_catalog,
            reveal_resource,
            authorize_mcp,
            sign_out_mcp,
            save_mcp,
            install_mcp,
            delete_mcp,
            set_mcp_enabled,
            get_mcp,
            get_mcp_json,
            get_mcp_inputs,
            save_mcp_inputs,
            list_mcp_tool_cache,
            schedule_mcp_tools_refresh,
            probe_mcp,
            list_skill_directory,
            read_skill_file,
            write_skill_file,
            skill_file_count,
            transfer_resource,
            save_agent,
            delete_agent,
            save_squad,
            delete_squad,
            delete_task,
            save_automation,
            delete_automation,
            run_automation,
            trigger_automation_event,
            receive_automation_webhook,
            list_automation_runs,
            list_webhook_deliveries,
            get_run_statistics,
            save_settings,
            check_git_executable,
            create_chat_session,
            update_chat_session_title,
            generate_chat_session_title,
            update_chat_session_target,
            update_task,
            update_task_meta,
            update_task_status,
            list_agent_runs,
            agent_system_context,
            list_runtime_models,
            list_task_runs,
            list_chat_messages,
            upsert_inbox_item,
            mark_inbox_read,
            mark_inbox_unread,
            mark_inbox_all_read,
            archive_inbox_item,
            delete_inbox_item,
            fork_chat_session,
            set_message_feedback,
            compact_chat_session,
            resolve_permission,
            load_attachment_preview,
            open_external_url,
            open_message_attachment,
            save_pasted_attachment,
            send_message,
            truncate_session_from_message,
            load_session_diff_data,
            load_active_diff_data,
            save_file_baseline,
            delete_file_baseline,
            get_snapshot_content,
            get_turn_snapshots,
            update_chunk_state,
            delete_snapshots_from,
            stop_run
        ])
        .run(tauri::generate_context!())
        .expect("启动 Multara 失败");
}
