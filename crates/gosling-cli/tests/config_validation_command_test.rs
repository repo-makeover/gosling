use std::process::{Command, Output};
use tempfile::TempDir;

fn gosling(root: &TempDir, config: &str) -> Output {
    let config_dir = root.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.yaml"), config).unwrap();

    Command::new(env!("CARGO_BIN_EXE_gosling"))
        .arg("doctor")
        .env("GOSLING_PATH_ROOT", root.path())
        .env("GOSLING_DISABLE_KEYRING", "1")
        .output()
        .expect("failed to run gosling binary")
}

#[test]
fn invalid_runtime_config_values_emit_actionable_warnings() {
    let cases = [
        ("GOSLING_MODE: yolo\n", "Invalid GOSLING_MODE"),
        ("GOSLING_MAX_TURNS: plenty\n", "Invalid GOSLING_MAX_TURNS"),
        (
            "GOSLING_AUTO_COMPACT_THRESHOLD: 5\n",
            "Invalid GOSLING_AUTO_COMPACT_THRESHOLD",
        ),
        (
            "GOSLING_AUTO_COMPACT_REDUCTION: 5\n",
            "Invalid GOSLING_AUTO_COMPACT_REDUCTION",
        ),
    ];

    for (config, expected_warning) in cases {
        let root = TempDir::new().unwrap();
        let output = gosling(&root, config);
        assert!(output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected_warning),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn valid_runtime_config_values_do_not_warn() {
    let root = TempDir::new().unwrap();
    let output = gosling(
        &root,
        "GOSLING_MODE: auto\nGOSLING_MAX_TURNS: 5\nGOSLING_AUTO_COMPACT_THRESHOLD: 0.8\nGOSLING_AUTO_COMPACT_REDUCTION: 0.15\n",
    );

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Invalid GOSLING_"));
}
