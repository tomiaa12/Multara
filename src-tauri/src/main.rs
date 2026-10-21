#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if multara::cli::should_run_as_cli(&args) {
        multara::cli::run();
    } else {
        multara::run();
    }
}

#[cfg(test)]
mod packaging_tests {
    use std::path::PathBuf;

    #[test]
    fn bundle_and_updater_are_configured() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let targets = value["bundle"]["targets"]
            .as_array()
            .expect("bundle.targets");
        for target in ["nsis", "msi", "dmg"] {
            assert!(
                targets.iter().any(|item| item == target),
                "缺少 {target} 打包目标"
            );
        }
        assert_eq!(value["bundle"]["createUpdaterArtifacts"], true);
        assert!(value["plugins"]["updater"]["endpoints"]
            .as_array()
            .expect("updater.endpoints")
            .iter()
            .any(|item| item
                .as_str()
                .is_some_and(|url| url.contains("Multara/releases/latest/download/latest.json"))));
    }

    #[test]
    fn desktop_bundle_icons_exist() {
        let icons = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons");
        for name in [
            "32x32.png",
            "128x128.png",
            "128x128@2x.png",
            "icon.ico",
            "icon.icns",
        ] {
            assert!(icons.join(name).is_file(), "缺少打包图标 {name}");
        }
    }

    #[test]
    fn main_window_stays_hidden_until_state_restore() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(value["app"]["windows"][0]["visible"], false);

        let windows: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.windows.conf.json")).unwrap();
        assert_eq!(windows["app"]["windows"][0]["visible"], false);
    }

    #[test]
    fn app_binary_with_cli_args_does_not_launch_the_window() {
        assert!(multara::cli::should_run_as_cli(&[
            "task".into(),
            "list".into()
        ]));
        assert!(multara::cli::should_run_as_cli(&["--help".into()]));
        assert!(multara::cli::should_run_as_cli(&[
            "--cli".into(),
            "project".into(),
            "list".into()
        ]));
        assert!(!multara::cli::should_run_as_cli(&[]));
        assert!(!multara::cli::should_run_as_cli(&["-psn_0_12345".into()]));
    }
}
