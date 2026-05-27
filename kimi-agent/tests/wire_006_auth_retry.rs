use kimi_agent::llm::is_auth_error;
use kosong::chat_provider::{ChatProviderError, ChatProviderErrorKind};

#[test]
fn test_is_auth_error_401() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(401), "Unauthorized");
    assert!(is_auth_error(&err));
}

#[test]
fn test_is_auth_error_403() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(403), "Forbidden");
    assert!(is_auth_error(&err));
}

#[test]
fn test_is_auth_error_message_contains_auth() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Other, "Authentication failed");
    assert!(is_auth_error(&err));
}

#[test]
fn test_is_auth_error_message_contains_unauthorized() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Other, "user is unauthorized");
    assert!(is_auth_error(&err));
}

#[test]
fn test_is_auth_error_non_auth_status() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(500), "Internal Server Error");
    assert!(!is_auth_error(&err));
}

#[test]
fn test_is_auth_error_non_auth_other() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Other, "Some random error");
    assert!(!is_auth_error(&err));
}

#[test]
fn test_is_auth_error_connection() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Connection, "Connection refused");
    assert!(!is_auth_error(&err));
}

#[test]
fn test_is_auth_error_429_not_auth() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(429), "Too Many Requests");
    assert!(!is_auth_error(&err));
}

#[test]
fn test_is_auth_error_504_not_auth() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(504), "Gateway Timeout");
    assert!(!is_auth_error(&err));
}
