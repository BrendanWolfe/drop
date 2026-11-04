// Exercise the application's actual encoder and error formatters without linking the GUI.
extern crate self as remote;
pub mod proxy {
    pub use proxy_auth::redact;
}
#[path = "../../remote/src/error.rs"]
#[allow(dead_code)]
mod error;
#[path = "../../src/logging.rs"]
mod logging;

use log4rs::encode::{Encode, writer::simple::SimpleWriter};
use proxy_auth::{Credentials, SecretHeader};

#[test]
fn raw_response_logs_and_error_display_debug_and_serialization_redact_secrets() {
    let _credentials = Credentials::new(
        &url::Url::parse("https://drop.example").unwrap(),
        vec![SecretHeader {
            name: "X-Token".into(),
            value: "logging-test-secret".into(),
        }],
    )
    .unwrap();
    let raw_body = "HTTP 400 proxy reflected X-Token: logging-test-secret";
    let mut output = Vec::new();
    logging::RedactingEncoder::new("{m}{n}")
        .encode(
            &mut SimpleWriter(&mut output),
            &log::Record::builder()
                .args(format_args!("{raw_body}"))
                .level(log::Level::Info)
                .build(),
        )
        .unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("HTTP 400 proxy reflected"));
    assert!(!output.contains("logging-test-secret"));

    for error in [
        error::RemoteAccessError::UnparseableResponse(raw_body.into()),
        error::RemoteAccessError::ManifestDownloadFailed(
            http::StatusCode::BAD_REQUEST,
            raw_body.into(),
        ),
        error::RemoteAccessError::InvalidResponse(error::DropServerError {
            status_code: 400,
            status_message: raw_body.into(),
            message: raw_body.into(),
        }),
    ] {
        for formatted in [
            error.to_string(),
            format!("{error:?}"),
            serde_json::to_string(&error).unwrap(),
        ] {
            assert!(!formatted.contains("logging-test-secret"));
            assert!(formatted.contains("REDACTED"));
        }
    }
}
