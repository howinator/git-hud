use crate::git::Status;
use anyhow::Result;
use colored::*;

pub struct StatusFormatter;

impl StatusFormatter {
    pub fn new() -> Self {
        Self
    }

    pub fn display(&self, status: &Status) -> Result<()> {
        // We'll implement this next
        todo!()
    }
}
