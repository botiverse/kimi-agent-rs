use std::collections::HashMap;
use std::path::PathBuf;

use kimi_agent::config::{
    Config, LoopControl, MCPConfig, Services, get_default_config, load_config,
    load_config_from_string, save_config,
};

#[test]
fn test_default_config() {
    let config = get_default_config();
    let expected = Config {
        is_from_default_location: false,
        source_file: None,
        default_model: String::new(),
        default_thinking: false,
        models: HashMap::new(),
        providers: HashMap::new(),
        loop_control: LoopControl::default(),
        services: Services::default(),
        mcp: MCPConfig::default(),
    };
    assert_eq!(config, expected);
}

#[test]
fn test_default_config_dump() {
    let config = get_default_config();
    let value = serde_json::to_value(&config).expect("serialize config");
    assert_eq!(
        value,
        serde_json::json!({
            "default_model": "",
            "default_thinking": false,
            "models": {},
            "providers": {},
            "loop_control": {
                "max_steps_per_turn": 1000,
                "max_retries_per_step": 3,
                "max_ralph_iterations": 0,
                "reserved_context_size": 50000,
            },
            "services": {
            },
            "mcp": {
                "client": {
                    "tool_call_timeout_ms": 60000,
                },
            },
        })
    );
}

#[test]
fn test_load_config_text_toml() {
    let config = load_config_from_string("default_model = \"\"").expect("load toml");
    assert_eq!(config, get_default_config());
}

#[test]
fn test_load_config_text_json() {
    let config = load_config_from_string("{\"default_model\": \"\"}").expect("load json");
    assert_eq!(config, get_default_config());
}

#[test]
fn test_load_config_text_invalid() {
    let err = load_config_from_string("not valid {").expect_err("invalid config");
    assert!(err.to_string().contains("Invalid configuration text"));
}

#[test]
fn test_load_config_text_invalid_compound_error_literal() {
    let err = load_config_from_string("not valid {").expect_err("invalid config");
    let err_msg = err.to_string();
    assert!(
        err_msg.starts_with("Invalid configuration text: "),
        "unexpected error format: {err_msg}"
    );
    assert!(
        err_msg.contains("(char "),
        "unexpected error format: {err_msg}"
    );
    assert!(
        err_msg.contains("; TOML parse error"),
        "unexpected error format: {err_msg}"
    );
}

#[test]
fn test_load_config_invalid_ralph_iterations() {
    let err = load_config_from_string("{\"loop_control\": {\"max_ralph_iterations\": -2}}")
        .expect_err("invalid ralph iterations");
    assert!(err.to_string().contains("max_ralph_iterations"));
}

#[test]
fn test_load_config_reserved_context_size() {
    let config = load_config_from_string("{\"loop_control\": {\"reserved_context_size\": 30000}}")
        .expect("load config");
    assert_eq!(config.loop_control.reserved_context_size, 30000);
}

#[test]
fn test_load_config_reserved_context_size_too_low() {
    let err = load_config_from_string("{\"loop_control\": {\"reserved_context_size\": 500}}")
        .expect_err("reserved_context_size too low");
    assert!(err.to_string().contains("reserved_context_size"));
}

#[tokio::test]
async fn test_load_config_sets_source_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("config.toml");
    tokio::fs::write(&config_path, "default_model = \"\"")
        .await
        .expect("write config");

    let config = load_config(Some(&config_path))
        .await
        .expect("load explicit config");
    assert_eq!(config.source_file.as_deref(), Some(config_path.as_path()));
}

#[tokio::test]
async fn test_save_config_skips_source_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("config.json");
    let mut config = get_default_config();
    config.source_file = Some(PathBuf::from("/tmp/from-elsewhere.toml"));

    save_config(&config, Some(&config_path))
        .await
        .expect("save config");

    let contents = tokio::fs::read_to_string(&config_path)
        .await
        .expect("read saved config");
    assert!(!contents.contains("source_file"));
}
