use serde_json::json;

#[test]
fn test_ensure_property_types_adds_string_default() {
    let mut schema = json!({
        "properties": {
            "name": {
                "description": "The name"
            }
        },
        "type": "object"
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(
        schema,
        json!({
            "properties": {
                "name": {
                    "description": "The name",
                    "type": "string"
                }
            },
            "type": "object"
        })
    );
}

#[test]
fn test_ensure_property_types_infers_from_enum() {
    let mut schema = json!({
        "properties": {
            "status": {
                "enum": ["pending", "in_progress", "done"]
            }
        },
        "type": "object"
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(
        schema["properties"]["status"]["type"],
        "string"
    );
}

#[test]
fn test_ensure_property_types_infers_from_anyof_null() {
    let mut schema = json!({
        "properties": {
            "directory": {
                "anyOf": [
                    {"type": "string"},
                    {"type": "null"}
                ],
                "default": null
            }
        },
        "type": "object"
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(
        schema["properties"]["directory"]["type"],
        "string"
    );
}

#[test]
fn test_ensure_property_types_infers_object_from_properties() {
    let mut schema = json!({
        "properties": {
            "nested": {
                "properties": {
                    "value": {"description": "A value"}
                }
            }
        },
        "type": "object"
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(
        schema["properties"]["nested"]["type"],
        "object"
    );
    assert_eq!(
        schema["properties"]["nested"]["properties"]["value"]["type"],
        "string"
    );
}

#[test]
fn test_ensure_property_types_infers_array_from_items() {
    let mut schema = json!({
        "properties": {
            "tags": {
                "items": {"description": "A tag"}
            }
        },
        "type": "object"
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(
        schema["properties"]["tags"]["type"],
        "array"
    );
    assert_eq!(
        schema["properties"]["tags"]["items"]["type"],
        "string"
    );
}

#[test]
fn test_ensure_property_types_preserves_existing_type() {
    let mut schema = json!({
        "properties": {
            "count": {
                "type": "integer",
                "minimum": 0
            }
        },
        "type": "object"
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    assert_eq!(
        schema["properties"]["count"]["type"],
        "integer"
    );
    assert_eq!(
        schema["properties"]["count"]["minimum"],
        0
    );
}

#[test]
fn test_ensure_property_types_deeply_nested() {
    let mut schema = json!({
        "properties": {
            "outer": {
                "properties": {
                    "inner": {
                        "items": {
                            "properties": {
                                "leaf": {"description": "leaf node"}
                            }
                        }
                    }
                }
            }
        },
        "type": "object"
    });

    kimi_agent::wire::ensure_property_types(&mut schema);

    // outer -> object (has properties)
    assert_eq!(schema["properties"]["outer"]["type"], "object");
    // inner -> array (has items)
    assert_eq!(schema["properties"]["outer"]["properties"]["inner"]["type"], "array");
    // inner.items -> object (has properties)
    assert_eq!(
        schema["properties"]["outer"]["properties"]["inner"]["items"]["type"],
        "object"
    );
    // leaf -> string (default)
    assert_eq!(
        schema["properties"]["outer"]["properties"]["inner"]["items"]["properties"]["leaf"]["type"],
        "string"
    );
}
