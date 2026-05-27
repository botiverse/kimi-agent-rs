use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use kimi_agent::llm::{is_auth_error, with_auth_retry};
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

#[tokio::test]
async fn test_with_auth_retry_succeeds_after_failures() {
    let counter = AtomicUsize::new(0);
    let result = with_auth_retry(
        || async {
            let count = counter.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Err(ChatProviderError::new(
                    ChatProviderErrorKind::Status(401),
                    "Unauthorized",
                ))
            } else {
                Ok("success")
            }
        },
        3,
        Duration::from_millis(10),
    )
    .await;

    assert_eq!(result.unwrap(), "success");
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_with_auth_retry_bounded_and_fails() {
    let counter = AtomicUsize::new(0);
    let result: Result<&str, _> = with_auth_retry(
        || async {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(ChatProviderError::new(
                ChatProviderErrorKind::Status(401),
                "Unauthorized",
            ))
        },
        2,
        Duration::from_millis(10),
    )
    .await;

    assert!(result.is_err());
    // initial attempt + 2 retries = 3 total calls
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_with_auth_retry_non_auth_error_not_retried() {
    let counter = AtomicUsize::new(0);
    let result: Result<&str, _> = with_auth_retry(
        || async {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(ChatProviderError::new(
                ChatProviderErrorKind::Status(500),
                "Internal Server Error",
            ))
        },
        3,
        Duration::from_millis(10),
    )
    .await;

    assert!(result.is_err());
    // Should only be called once because 500 is not an auth error
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_with_auth_retry_other_kind_auth_message_retried() {
    let counter = AtomicUsize::new(0);
    let result = with_auth_retry(
        || async {
            let count = counter.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                Err(ChatProviderError::new(
                    ChatProviderErrorKind::Other,
                    "token refresh required: auth expired",
                ))
            } else {
                Ok("recovered")
            }
        },
        3,
        Duration::from_millis(10),
    )
    .await;

    assert_eq!(result.unwrap(), "recovered");
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}
