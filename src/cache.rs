use crate::error::HudError;
use std::path::PathBuf;
use tempfile::NamedTempFile;

pub struct Cache {
    file: NamedTempFile,
}

impl Cache {
    pub fn new() -> Result<Self, HudError> {
        let file = NamedTempFile::new()?;
        Ok(Self { file })
    }

    pub fn get(&self, key: &str) -> Option<String> {
        // We'll implement this next
        todo!()
    }

    pub fn set(&mut self, key: &str, value: String) -> Result<(), HudError> {
        // We'll implement this next
        todo!()
    }
}
