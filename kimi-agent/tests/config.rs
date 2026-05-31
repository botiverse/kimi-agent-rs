use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use kimi_agent::config::{
    Config, LLMModel, LLMProvider, LoopControl, MCPConfig, ModelCapability, MoonshotSearchConfig,
    ProviderType, SecretString, Services, get_default_config, load_config, load_config_from_string,
    save_config,
};

fn collect_null_leaf_paths(value: &serde_json::Value, path: &str, paths: &mut Vec<String>) {
    match value {
        serde_json::Value::Null => paths.push(path.to_string()),
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{path}.{key}");
                collect_null_leaf_paths(child, &child_path, paths);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let child_path = format!("{path}[{index}]");
                collect_null_leaf_paths(child, &child_path, paths);
            }
        }
        _ => {}
    }
}

fn assert_no_null_leaves(value: &serde_json::Value) {
    let mut null_paths = Vec::new();
    collect_null_leaf_paths(value, "$", &mut null_paths);
    assert!(
        null_paths.is_empty(),
        "serialized config should not contain JSON null leaves, found at: {null_paths:?}"
    );
}

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
    assert_eq!(
        err_msg,
        "Invalid configuration text: Expecting value: line 1 column 1 (char 0); Invalid key \"not valid\" at line 1 col 10",
        "unexpected error format: {err_msg}"
    );
}

#[test]
fn test_load_config_text_invalid_compound_error_literal_single_key() {
    let err = load_config_from_string("foo {").expect_err("invalid config");
    let err_msg = err.to_string();
    assert_eq!(
        err_msg,
        "Invalid configuration text: Expecting value: line 1 column 1 (char 0); Invalid key \"foo\" at line 1 col 4",
        "unexpected error format: {err_msg}"
    );
}

#[test]
fn test_load_config_text_invalid_compound_error_literal_json_key_shape() {
    let err = load_config_from_string("{foo=1}").expect_err("invalid config");
    let err_msg = err.to_string();
    assert!(
        err_msg.starts_with(
            "Invalid configuration text: Expecting property name enclosed in double quotes: line 1 column 2 (char 1); "
        ),
        "unexpected error format: {err_msg}"
    );
    assert!(
        err_msg.contains("TOML parse error at line 1, column 1"),
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

#[tokio::test]
async fn test_save_config_excludes_nested_none_fields_in_json() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("config.json");
    let mut config = get_default_config();

    config.default_model = "kimi".to_string();
    config.providers.insert(
        "moonshot".to_string(),
        LLMProvider {
            provider_type: ProviderType::Kimi,
            base_url: "https://api.moonshot.ai/v1/null-safe".to_string(),
            api_key: SecretString::new("sk-null-token"),
            env: None,
            custom_headers: None,
        },
    );
    config.models.insert(
        "kimi".to_string(),
        LLMModel {
            provider: "moonshot".to_string(),
            model: "kimi-k2".to_string(),
            max_context_size: 128_000,
            capabilities: Some(HashSet::from([ModelCapability::Thinking])),
        },
    );
    config.services = Services {
        moonshot_search: Some(MoonshotSearchConfig {
            base_url: "https://search.moonshot.ai/v1".to_string(),
            api_key: SecretString::new("search-null-key"),
            custom_headers: None,
        }),
        moonshot_fetch: None,
    };

    save_config(&config, Some(&config_path))
        .await
        .expect("save config");

    let contents = tokio::fs::read_to_string(&config_path)
        .await
        .expect("read saved config");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    assert_no_null_leaves(&value);
    assert_eq!(
        value["providers"]["moonshot"],
        serde_json::json!({
            "type": "kimi",
            "base_url": "https://api.moonshot.ai/v1/null-safe",
            "api_key": "sk-null-token"
        })
    );
    assert_eq!(
        value["services"],
        serde_json::json!({
            "moonshot_search": {
                "base_url": "https://search.moonshot.ai/v1",
                "api_key": "search-null-key"
            }
        })
    );
}

#[tokio::test]
async fn test_save_config_roundtrip_strips_explicit_null_service_fields() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("config.json");
    let config =
        load_config_from_string("{\"services\":{\"moonshot_search\":null,\"moonshot_fetch\":null}}")
            .expect("load config with explicit null service fields");

    assert!(config.services.moonshot_search.is_none());
    assert!(config.services.moonshot_fetch.is_none());

    save_config(&config, Some(&config_path))
        .await
        .expect("save config");

    let contents = tokio::fs::read_to_string(&config_path)
        .await
        .expect("read saved config");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    assert_no_null_leaves(&value);
    assert_eq!(value["services"], serde_json::json!({}));
}
