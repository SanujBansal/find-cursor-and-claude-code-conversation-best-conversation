mod analytics;
mod azure;
mod commands;
mod db;
mod exporters;
mod importers;
mod rules;
mod scoring;

use commands::{
    clear_all_transcripts, export_conversation_markdown, get_conversation_export_markdown,
    get_conversation_messages, get_dashboard, get_default_cursor_path, get_import_status,
    get_project_top_conversations, get_scores, get_settings, get_top_conversations, get_trend_data,
    import_all, import_all_and_score, import_claude_code, import_claude_markdown, import_cursor,
    list_conversations, list_projects, refresh_analytics, save_settings, scan_project_rules,
    score_conversation, score_pending, score_project, score_project_rules,
};
use db::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let database = Database::initialize(app.handle())?;
            app.manage(database);

            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;
            app.handle().plugin(tauri_plugin_dialog::init())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            list_conversations,
            list_projects,
            get_settings,
            save_settings,
            import_cursor,
            import_claude_code,
            import_claude_markdown,
            import_all,
            import_all_and_score,
            clear_all_transcripts,
            get_import_status,
            get_default_cursor_path,
            score_pending,
            score_project,
            score_conversation,
            get_scores,
            get_top_conversations,
            get_project_top_conversations,
            get_conversation_messages,
            get_conversation_export_markdown,
            export_conversation_markdown,
            refresh_analytics,
            get_trend_data,
            scan_project_rules,
            score_project_rules
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
