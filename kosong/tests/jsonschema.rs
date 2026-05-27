use kosong::utils::jsonschema::ensure_property_types;
use serde_json::json;

#[test]
fn test_ensure_property_types_fills_missing_type_on_enum() {
    let schema = json!({
        "type": "object",
        "properties": {
            "truncateMode": {
                "description": "How to truncate long outputs.",
                "enum": ["smart", "full", "none"],
            }
        },
    });
    let expected = json!({
        "type": "object",
        "properties": {
            "truncateMode": {
                "description": "How to truncate long outputs.",
                "enum": ["smart", "full", "none"],
                "type": "string",
            }
        },
    });
    assert_eq!(ensure_property_types(&schema), expected);
}

#[test]
fn test_ensure_property_types_does_not_mutate_input() {
    let schema = json!({
        "type": "object",
        "properties": {"x": {"enum": ["a", "b"]}},
    });
    let original = schema.clone();
    let _ = ensure_property_types(&schema);
    assert_eq!(schema, original);
}

#[test]
fn test_ensure_property_types_infers_from_enum_values() {
    let schema = json!({
        "type": "object",
        "properties": {
            "as_string": {"enum": ["a", "b"]},
            "as_integer": {"enum": [1, 2, 3]},
            "int_and_float_as_number": {"enum": [1.0, 2]},
            "as_boolean": {"enum": [true, false]},
            "as_null": {"enum": [null]},
            "bool_and_int_fallback": {"enum": [true, 1]},
            "string_and_int_fallback": {"enum": ["a", 1]},
        },
    });
    let expected = json!({
        "type": "object",
        "properties": {
            "as_string": {"enum": ["a", "b"], "type": "string"},
            "as_integer": {"enum": [1, 2, 3], "type": "integer"},
            "int_and_float_as_number": {"enum": [1.0, 2], "type": "number"},
            "as_boolean": {"enum": [true, false], "type": "boolean"},
            "as_null": {"enum": [null], "type": "null"},
            "bool_and_int_fallback": {"enum": [true, 1], "type": "string"},
            "string_and_int_fallback": {"enum": ["a", 1], "type": "string"},
        },
    });
    assert_eq!(ensure_property_types(&schema), expected);
}

#[test]
fn test_ensure_property_types_handles_const() {
    let schema = json!({
        "type": "object",
        "properties": {"kind": {"const": "event"}},
    });
    let expected = json!({
        "type": "object",
        "properties": {"kind": {"const": "event", "type": "string"}},
    });
    assert_eq!(ensure_property_types(&schema), expected);
}

#[test]
fn test_ensure_property_types_defaults_to_string_when_no_hint() {
    let schema = json!({
        "type": "object",
        "properties": {"opaque": {"description": "Some value."}},
    });
    let expected = json!({
        "type": "object",
        "properties": {"opaque": {"description": "Some value.", "type": "string"}},
    });
    assert_eq!(ensure_property_types(&schema), expected);
}

#[test]
fn test_ensure_property_types_infers_structural_type_before_string_fallback() {
    let schema = json!({
        "type": "object",
        "properties": {
            "nested_object": {
                "properties": {"host": {"type": "string"}},
                "required": ["host"],
            },
            "free_form_map": {"additionalProperties": {"type": "string"}},
            "list_of_ints": {"items": {"type": "integer"}},
            "bounded_list": {"minItems": 1, "maxItems": 10},
            "email": {"format": "email"},
            "slug": {"pattern": "^[a-z0-9-]+$"},
            "bounded_number": {"minimum": 0, "maximum": 100},
            "opaque": {"description": "something"},
        },
    });
    let expected = json!({
        "type": "object",
        "properties": {
            "nested_object": {
                "properties": {"host": {"type": "string"}},
                "required": ["host"],
                "type": "object",
            },
            "free_form_map": {
                "additionalProperties": {"type": "string"},
                "type": "object",
            },
            "list_of_ints": {"items": {"type": "integer"}, "type": "array"},
            "bounded_list": {"minItems": 1, "maxItems": 10, "type": "array"},
            "email": {"format": "email", "type": "string"},
            "slug": {"pattern": "^[a-z0-9-]+$", "type": "string"},
            "bounded_number": {"minimum": 0, "maximum": 100, "type": "number"},
            "opaque": {"description": "something", "type": "string"},
        },
    });
    assert_eq!(ensure_property_types(&schema), expected);
}

#[test]
fn test_ensure_property_types_leaves_combinators_alone() {
    let schema = json!({
        "type": "object",
        "properties": {
            "either": {
                "anyOf": [
                    {"type": "string"},
                    {"enum": [1, 2]},
                ]
            },
            "ref_prop": {"$ref": "#/$defs/Something"},
            "negated": {"not": {"type": "number"}},
            "conditional": {
                "if": {"properties": {"kind": {"const": "a"}}},
                "then": {"required": ["a_only"]},
                "else": {"required": ["b_only"]},
            },
        },
    });
    let expected = json!({
        "type": "object",
        "properties": {
            "either": {
                "anyOf": [
                    {"type": "string"},
                    {"enum": [1, 2], "type": "integer"},
                ]
            },
            "ref_prop": {"$ref": "#/$defs/Something"},
            "negated": {"not": {"type": "number"}},
            "conditional": {
                "if": {"properties": {"kind": {"const": "a"}}},
                "then": {"required": ["a_only"]},
                "else": {"required": ["b_only"]},
            },
        },
    });
    assert_eq!(ensure_property_types(&schema), expected);
}

#[test]
fn test_ensure_property_types_infers_object_and_array_from_container_enum_values() {
    let schema = json!({
        "type": "object",
        "properties": {
            "object_enum": {"enum": [{"a": 1}, {"a": 2}]},
            "array_enum": {"enum": [[1, 2], [3]]},
            "object_const": {"const": {"kind": "default"}},
            "array_const": {"const": []},
        },
    });
    let expected = json!({
        "type": "object",
        "properties": {
            "object_enum": {"enum": [{"a": 1}, {"a": 2}], "type": "object"},
            "array_enum": {"enum": [[1, 2], [3]], "type": "array"},
            "object_const": {"const": {"kind": "default"}, "type": "object"},
            "array_const": {"const": [], "type": "array"},
        },
    });
    assert_eq!(ensure_property_types(&schema), expected);
}

#[test]
fn test_ensure_property_types_recurses_into_nested_objects_and_arrays() {
    let schema = json!({
        "type": "object",
        "properties": {
            "nested": {
                "type": "object",
                "properties": {
                    "choice": {"enum": ["a", "b"]},
                },
            },
            "items_list": {
                "type": "array",
                "items": {"enum": [1, 2, 3]},
            },
            "free_map": {
                "type": "object",
                "additionalProperties": {"enum": ["x", "y"]},
            },
        },
    });
    let expected = json!({
        "type": "object",
        "properties": {
            "nested": {
                "type": "object",
                "properties": {
                    "choice": {"enum": ["a", "b"], "type": "string"},
                },
            },
            "items_list": {
                "type": "array",
                "items": {"enum": [1, 2, 3], "type": "integer"},
            },
            "free_map": {
                "type": "object",
                "additionalProperties": {"enum": ["x", "y"], "type": "string"},
            },
        },
    });
    assert_eq!(ensure_property_types(&schema), expected);
}
