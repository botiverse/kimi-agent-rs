use serde_json::json;

#[test]
fn test_ensure_property_types_coerces_string_to_array() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string"
            }
        }
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(
        schema,
        json!({
            "type": ["object"],
            "properties": {
                "name": {
                    "type": ["string"]
                }
            }
        })
    );
}

#[test]
fn test_ensure_property_types_passes_through_array() {
    let mut schema = json!({
        "type": ["string", "null"]
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(
        schema,
        json!({
            "type": ["string", "null"]
        })
    );
}

#[test]
fn test_ensure_property_types_nested_properties() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "id": {"type": "number"}
                }
            }
        }
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(schema["type"], json!(["object"]));
    assert_eq!(schema["properties"]["user"]["type"], json!(["object"]));
    assert_eq!(schema["properties"]["user"]["properties"]["id"]["type"], json!(["number"]));
}

#[test]
fn test_ensure_property_types_array_items() {
    let mut schema = json!({
        "type": "array",
        "items": {"type": "string"}
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(schema["type"], json!(["array"]));
    assert_eq!(schema["items"]["type"], json!(["string"]));
}

#[test]
fn test_ensure_property_types_additional_properties() {
    let mut schema = json!({
        "type": "object",
        "additionalProperties": {"type": "boolean"}
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(schema["type"], json!(["object"]));
    assert_eq!(schema["additionalProperties"]["type"], json!(["boolean"]));
}

#[test]
fn test_ensure_property_types_allof() {
    let mut schema = json!({
        "allOf": [
            {"type": "object"},
            {"type": "string"}
        ]
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(schema["allOf"][0]["type"], json!(["object"]));
    assert_eq!(schema["allOf"][1]["type"], json!(["string"]));
}

#[test]
fn test_ensure_property_types_anyof() {
    let mut schema = json!({
        "anyOf": [
            {"type": "string"},
            {"type": "null"}
        ]
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(schema["anyOf"][0]["type"], json!(["string"]));
    assert_eq!(schema["anyOf"][1]["type"], json!(["null"]));
}
