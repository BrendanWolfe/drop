use log4rs::encode::{Encode, Write, pattern::PatternEncoder, writer::simple::SimpleWriter};

/// Redact the fully formatted record, including raw response bodies and Debug/source chains.
/// Both appenders use this boundary; callers cannot accidentally bypass it with a log macro.
#[derive(Debug)]
pub struct RedactingEncoder(PatternEncoder);

impl RedactingEncoder {
    pub fn new(pattern: &str) -> Self {
        Self(PatternEncoder::new(pattern))
    }
}

impl Encode for RedactingEncoder {
    fn encode(&self, writer: &mut dyn Write, record: &log::Record<'_>) -> anyhow::Result<()> {
        let mut buffer = Vec::new();
        self.0.encode(&mut SimpleWriter(&mut buffer), record)?;
        writer.write_all(remote::proxy::redact(&String::from_utf8_lossy(&buffer)).as_bytes())?;
        Ok(())
    }
}
