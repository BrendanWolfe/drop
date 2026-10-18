use std::fmt::{Display, Formatter};

use log::error;
use reqwest::StatusCode;

#[derive(Debug)]
pub struct ServiceError {
    status_code: Option<StatusCode>,
    message: String,
}

impl ServiceError {
    #[must_use]
    pub fn from_payload(payload: String) -> Self {
        #[derive(serde::Deserialize)]
        struct Payload {
            #[serde(rename = "statusCode")]
            status_code: Option<u16>,
            message: Option<String>,
        }

        if let Ok(payload_details) = serde_json::from_str::<Payload>(&payload)
            && (payload_details.status_code.is_some() || payload_details.message.is_some())
        {
            return Self {
                status_code: payload_details
                    .status_code
                    .and_then(|code| StatusCode::from_u16(code).ok()),
                message: payload_details.message.unwrap_or(payload),
            };
        }

        Self {
            status_code: None,
            message: payload,
        }
    }

    #[must_use]
    pub fn status_code(&self) -> Option<StatusCode> {
        self.status_code
    }
}

impl Display for ServiceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ServiceError {}

#[derive(Debug)]
pub struct ErrorOption(Result<StatusCode, anyhow::Error>);
impl From<anyhow::Error> for ErrorOption {
    fn from(value: anyhow::Error) -> Self {
        if let Some(service_error) = value.downcast_ref::<ServiceError>()
            && let Some(status_code) = service_error.status_code()
        {
            return Self(Ok(status_code));
        }
        Self(Err(value))
    }
}
impl From<reqwest::Error> for ErrorOption {
    fn from(value: reqwest::Error) -> Self {
        Self(Err(value.into()))
    }
}
impl From<url::ParseError> for ErrorOption {
    fn from(value: url::ParseError) -> Self {
        Self(Err(value.into()))
    }
}
impl From<StatusCode> for ErrorOption {
    fn from(value: StatusCode) -> Self {
        Self(Ok(value))
    }
}

impl From<ErrorOption> for StatusCode {
    fn from(value: ErrorOption) -> Self {
        match value.0 {
            Ok(status) => status,
            Err(err) => {
                error!("{err:?}");
                Self::INTERNAL_SERVER_ERROR
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_status_is_preserved() {
        let error: anyhow::Error = ServiceError::from_payload(
            r#"{"statusCode":404,"message":"Game version not found"}"#.to_owned(),
        )
        .into();

        let status = StatusCode::from(ErrorOption::from(error));

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn unstructured_error_remains_internal_server_error() {
        let error: anyhow::Error =
            ServiceError::from_payload("Game version not found".to_owned()).into();

        let status = StatusCode::from(ErrorOption::from(error));

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn invalid_structured_status_remains_internal_server_error() {
        let error: anyhow::Error =
            ServiceError::from_payload(r#"{"statusCode":99,"message":"invalid"}"#.to_owned())
                .into();

        let status = StatusCode::from(ErrorOption::from(error));

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
