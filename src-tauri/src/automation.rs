use std::{thread, time::Duration};

use tauri::{AppHandle, Manager};

use crate::{
    commands::{due_automations, execute_scheduled_automation},
    database::AppState,
};

pub fn start(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(30));
        let _span = crate::diag::span("automation_tick");
        let ids = {
            let state = app.state::<AppState>();
            let Ok(db) = state.lock_db() else {
                continue;
            };
            due_automations(&db).unwrap_or_default()
        };
        for due in ids {
            let _ = execute_scheduled_automation(&app, &due);
        }
    });
}
