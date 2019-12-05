use std::{fmt, time::Instant};

pub type Error = anyhow::Error;
pub type Result<T> = anyhow::Result<T>;

#[derive(Debug, Error)]
pub struct DescError(std::borrow::Cow<'static, str>);

impl fmt::Display for DescError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&'static str> for DescError {
    fn from(s: &'static str) -> Self {
        Self(s.into())
    }
}

impl From<String> for DescError {
    fn from(s: String) -> Self {
        Self(s.into())
    }
}

impl AsRef<str> for DescError {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
