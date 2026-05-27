use serde_json::Value;

/// JSON Schema keywords that describe a property's shape without (or in
/// addition to) a `type` keyword. When any of these are present we skip
/// the type-filling step so we don't distort the schema's meaning —
/// `not`/`if`/`then`/`else` are less common but every bit as valid
/// as `anyOf`/`oneOf`/`allOf`.
const COMBINATOR_KEYS: &[&str] = &[
    "anyOf", "oneOf", "allOf", "not", "if", "then", "else", "$ref",
];

/// Structural keywords that only make sense for a given JSON Schema type.
/// Used to infer `type` when enum/const are absent but the node otherwise
/// clearly describes an object or array or constrained scalar — setting
/// `type: "string"` on such a node would misadvertise the parameter shape
/// and cause the model to emit arguments that then fail downstream
/// `jsonschema.validate` against the tool's real parameter schema.
const OBJECT_KEYWORDS: &[&str] = &[
    "properties",
    "additionalProperties",
    "patternProperties",
    "propertyNames",
    "required",
    "minProperties",
    "maxProperties",
];
const ARRAY_KEYWORDS: &[&str] = &[
    "items", "prefixItems", "minItems", "maxItems", "uniqueItems", "contains",
];
const STRING_KEYWORDS: &[&str] = &["minLength", "maxLength", "pattern", "format"];
const NUMERIC_KEYWORDS: &[&str] = &[
    "minimum",
    "maximum",
    "multipleOf",
    "exclusiveMinimum",
    "exclusiveMaximum",
];

/// Expand local `$ref` entries in a JSON Schema without infinite recursion.
pub fn deref_json_schema(schema: &Value) -> Value {
    let mut full = schema.clone();
    let root = full.clone();
    let resolved = traverse(&mut full, &root);
    let mut cleaned = resolved;
    if let Value::Object(map) = &mut cleaned {
        map.remove("$defs");
        map.remove("definitions");
    }
    cleaned
}

fn resolve_pointer(root: &Value, pointer: &str) -> Option<Value> {
    let mut current = root;
    let trimmed = pointer.trim_start_matches('#').trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(root.clone());
    }
    for part in trimmed.split('/') {
        match current {
            Value::Object(map) => {
                current = map.get(part)?;
            }
            _ => return None,
        }
    }
    Some(current.clone())
}

fn traverse(node: &mut Value, root: &Value) -> Value {
    match node {
        Value::Object(map) => {
            if let Some(Value::String(ref_path)) = map.get("$ref") {
                if ref_path.starts_with('#') {
                    if let Some(target) = resolve_pointer(root, ref_path) {
                        let mut resolved = target;
                        resolved = traverse(&mut resolved, root);
                        if let Value::Object(_) = resolved {
                            map.remove("$ref");
                            if let Value::Object(target_map) = resolved {
                                for (k, v) in target_map {
                                    map.insert(k, v);
                                }
                            }
                            return Value::Object(map.clone());
                        }
                    }
                }
            }
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(mut value) = map.remove(&key) {
                    let new_value = traverse(&mut value, root);
                    map.insert(key, new_value);
                }
            }
            Value::Object(map.clone())
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for mut item in items.clone() {
                out.push(traverse(&mut item, root));
            }
            Value::Array(out)
        }
        _ => node.clone(),
    }
}

/// Return a deep copy of `schema` with an explicit `type` on every property.
///
/// The Moonshot (Kimi) API rejects tool parameter schemas where a property
/// schema omits `type` — for example `{"enum": ["smart", "full"]}` with no
/// `"type": "string"`. JSON Schema itself permits this (the property then
/// accepts any value), and providers such as OpenAI and Anthropic accept it,
/// but Moonshot's stricter validator returns HTTP 400 with
/// `"At path 'properties.X': type is not defined"`.
///
/// This function walks any property schemas nested under `properties`,
/// `items`, `additionalProperties`, `anyOf`, `oneOf`, and `allOf`
/// and fills in a `type` when one is missing:
///
/// - when `enum` / `const` is present, the type is inferred from the values
/// - otherwise the type defaults to `"string"`
///
/// Nodes that use combinators (`anyOf`/`oneOf`/`allOf`/`$ref`/etc.) are left
/// alone since they legitimately declare their shape without `type`. The
/// outer schema object itself is treated as a container and never mutated —
/// only the property schemas it contains are normalized.
pub fn ensure_property_types(schema: &Value) -> Value {
    let mut result = schema.clone();
    recurse_schema(&mut result);
    result
}

fn recurse_schema(node: &mut Value) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };

    if let Some(Value::Object(props)) = obj.get_mut("properties") {
        for value in props.values_mut() {
            normalize_property(value);
        }
    }

    if let Some(items) = obj.get_mut("items") {
        match items {
            Value::Object(_) => normalize_property(items),
            Value::Array(arr) => {
                for value in arr.iter_mut() {
                    normalize_property(value);
                }
            }
            _ => {}
        }
    }

    if let Some(additional) = obj.get_mut("additionalProperties") {
        if additional.is_object() {
            normalize_property(additional);
        }
    }

    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(branches)) = obj.get_mut(key) {
            for value in branches.iter_mut() {
                normalize_property(value);
            }
        }
    }
}

fn normalize_property(node: &mut Value) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };

    if !obj.contains_key("type")
        && !COMBINATOR_KEYS.iter().any(|k| obj.contains_key(*k))
    {
        if let Some(Value::Array(enum_values)) = obj.get("enum") {
            if !enum_values.is_empty() {
                obj.insert(
                    "type".to_string(),
                    Value::String(infer_type_from_values(enum_values).to_string()),
                );
            }
        } else if obj.contains_key("const") {
            if let Some(const_value) = obj.get("const") {
                obj.insert(
                    "type".to_string(),
                    Value::String(infer_type_from_values(&[const_value.clone()]).to_string()),
                );
            }
        } else {
            obj.insert(
                "type".to_string(),
                Value::String(infer_type_from_structure(obj).to_string()),
            );
        }
    }

    recurse_schema(node);
}

fn infer_type_from_structure(node: &serde_json::Map<String, Value>) -> &'static str {
    if OBJECT_KEYWORDS.iter().any(|k| node.contains_key(*k)) {
        return "object";
    }
    if ARRAY_KEYWORDS.iter().any(|k| node.contains_key(*k)) {
        return "array";
    }
    if STRING_KEYWORDS.iter().any(|k| node.contains_key(*k)) {
        return "string";
    }
    if NUMERIC_KEYWORDS.iter().any(|k| node.contains_key(*k)) {
        return "number";
    }
    "string"
}

fn infer_type_from_values(values: &[Value]) -> &'static str {
    use std::collections::HashSet;

    let mut inferred: HashSet<&'static str> = HashSet::new();
    for value in values {
        let ty = match value {
            Value::Bool(_) => "boolean",
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    "integer"
                } else {
                    "number"
                }
            }
            Value::String(_) => "string",
            Value::Null => "null",
            Value::Object(_) => "object",
            Value::Array(_) => "array",
        };
        inferred.insert(ty);
    }

    if inferred.len() == 1 {
        return inferred.into_iter().next().unwrap();
    }
    if inferred == ["integer", "number"].iter().cloned().collect() {
        return "number";
    }
    "string"
}
