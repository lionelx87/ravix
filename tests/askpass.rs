use std::os::unix::net::UnixListener;
use std::thread;

use ravix::askpass::{
    CredentialCache, PromptKind, handle_connection, parse_askpass_prompt, request_credential,
};
use ravix::remote::is_auth_failure;
use tempfile::tempdir;

#[test]
fn a_password_prompt_is_classified_and_carries_its_host() {
    let prompt = parse_askpass_prompt("Password for 'https://CPSAA@dev.azure.com': ");
    assert_eq!(prompt.kind, PromptKind::Password);
    assert_eq!(prompt.host, "https://CPSAA@dev.azure.com");
}

#[test]
fn a_username_prompt_is_classified_and_carries_its_host() {
    let prompt = parse_askpass_prompt("Username for 'https://dev.azure.com': ");
    assert_eq!(prompt.kind, PromptKind::Username);
    assert_eq!(prompt.host, "https://dev.azure.com");
}

#[test]
fn a_prompt_without_a_quoted_host_still_classifies() {
    let prompt = parse_askpass_prompt("Password: ");
    assert_eq!(prompt.kind, PromptKind::Password);
    assert_eq!(prompt.host, "");
}

#[test]
fn an_unrecognised_prompt_defaults_to_a_masked_password() {
    let prompt = parse_askpass_prompt("Enter your token: ");
    assert_eq!(prompt.kind, PromptKind::Password);
}

#[test]
fn an_unseen_host_has_no_cached_credential() {
    let cache = CredentialCache::default();
    assert_eq!(
        cache.get("https://dev.azure.com", PromptKind::Password),
        None
    );
}

#[test]
fn a_stored_credential_is_returned_for_the_same_host_and_kind() {
    let mut cache = CredentialCache::default();
    cache.store(
        "https://dev.azure.com",
        PromptKind::Password,
        "s3cr3t".to_string(),
    );
    assert_eq!(
        cache.get("https://dev.azure.com", PromptKind::Password),
        Some("s3cr3t")
    );
}

#[test]
fn the_cache_key_separates_hosts_and_kinds() {
    let mut cache = CredentialCache::default();
    cache.store(
        "https://dev.azure.com",
        PromptKind::Password,
        "pass".to_string(),
    );
    cache.store("https://dev.azure.com", PromptKind::Username, "user".to_string());

    assert_eq!(
        cache.get("https://dev.azure.com", PromptKind::Username),
        Some("user")
    );
    assert_eq!(
        cache.get("https://dev.azure.com", PromptKind::Password),
        Some("pass")
    );
    assert_eq!(cache.get("https://github.com", PromptKind::Password), None);
}

#[test]
fn invalidating_a_host_forgets_all_its_kinds_but_leaves_other_hosts() {
    let mut cache = CredentialCache::default();
    cache.store("https://dev.azure.com", PromptKind::Username, "user".to_string());
    cache.store("https://dev.azure.com", PromptKind::Password, "pass".to_string());
    cache.store("https://github.com", PromptKind::Password, "ghp".to_string());

    cache.invalidate("https://dev.azure.com");

    assert_eq!(cache.get("https://dev.azure.com", PromptKind::Username), None);
    assert_eq!(cache.get("https://dev.azure.com", PromptKind::Password), None);
    assert_eq!(
        cache.get("https://github.com", PromptKind::Password),
        Some("ghp")
    );
}

#[test]
fn authentication_failures_are_recognised() {
    assert!(is_auth_failure(
        "fatal: Authentication failed for 'https://dev.azure.com/org/_git/repo'"
    ));
    assert!(is_auth_failure("remote: Invalid username or password"));
    assert!(is_auth_failure(
        "fatal: could not read Password for 'https://x': terminal prompts disabled"
    ));
}

#[test]
fn unrelated_failures_are_not_authentication_failures() {
    assert!(!is_auth_failure(
        "fatal: unable to access '...': Could not resolve host: dev.azure.com"
    ));
    assert!(!is_auth_failure(
        " ! [rejected] main -> main (non-fast-forward)"
    ));
}

#[test]
fn a_credential_typed_into_the_modal_travels_back_over_the_socket() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ravix.sock");
    let listener = UnixListener::bind(&path).unwrap();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        handle_connection(stream, |prompt| {
            assert!(prompt.contains("Password"));
            "hunter2".to_string()
        })
        .unwrap();
    });

    let answer = request_credential(&path, "Password for 'https://dev.azure.com': ").unwrap();
    assert_eq!(answer, "hunter2");

    server.join().unwrap();
}

#[test]
fn an_empty_answer_round_trips_as_a_cancelled_credential() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ravix.sock");
    let listener = UnixListener::bind(&path).unwrap();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        handle_connection(stream, |_| String::new()).unwrap();
    });

    let answer = request_credential(&path, "Password for 'https://dev.azure.com': ").unwrap();
    assert_eq!(answer, "");

    server.join().unwrap();
}
