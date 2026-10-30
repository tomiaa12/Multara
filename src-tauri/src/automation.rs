use std::{
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};

use crate::{
    commands::{due_automations, execute_scheduled_automation},
    database::{self, AppState},
};

const TICK: Duration = Duration::from_secs(30);
const OPTIMIZE_INTERVAL: Duration = Duration::from_secs(60 * 60);

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let mut last_optimize = Instant::now();
        loop {
            thread::sleep(TICK);
            let _span = crate::diag::span("automation_tick");
            let ids = {
                let state = app.state::<AppState>();
                let Ok(db) = state.lock_db() else {
                    continue;
                };
                if last_optimize.elapsed() >= OPTIMIZE_INTERVAL {
                    last_optimize = Instant::now();
                    database::optimize(&db);
                }
                due_automations(&db).unwrap_or_default()
            };
            for due in ids {
                let _ = execute_scheduled_automation(&app, &due);
            }
        }
    });
}
