//! Turns the id flags into a concrete list of ids, and defines the usage
//! error the binary maps to exit code 1.

use std::fmt;

use crate::cache::IdArgs;

/// A contradictory or incomplete selection.
///
/// The binary looks for this inside an `anyhow::Error` and exits 1 instead
/// of 2. The string is the message shown to you.
#[derive(Debug)]
pub struct UsageError(pub String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

/// Wraps `msg` in a [`UsageError`], ready to return with `?`.
pub fn usage(msg: impl Into<String>) -> anyhow::Error {
    UsageError(msg.into()).into()
}

impl IdArgs {
    /// Whether any of `--id`, `--range` or `--all` was set.
    pub fn given(&self) -> bool {
        self.id.is_some() || self.range.is_some() || self.all
    }

    /// Picks ids out of `available` (ascending), or fails with a usage
    /// error naming `what` when no id flag was set.
    ///
    /// `--id` comes back as given, even if `available` doesn't have it.
    /// The dump then finds it missing and counts it as absent in the
    /// manifest, which isn't an error. `--range` and `--all` only ever
    /// return ids that are in `available`.
    pub fn resolve(&self, what: &str, available: &[u32]) -> anyhow::Result<Vec<u32>> {
        if let Some(id) = self.id {
            return Ok(vec![id]);
        }
        if let Some((a, b)) = self.range {
            return Ok(available
                .iter()
                .copied()
                .filter(|i| (a..=b).contains(i))
                .collect());
        }
        if self.all {
            return Ok(available.to_vec());
        }
        Err(usage(format!("choose --id, --range or --all for {what}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_ids() {
        let have = [1, 2, 5, 9];
        let ids = |id, range, all| IdArgs { id, range, all };
        assert_eq!(ids(Some(7), None, false).resolve("x", &have).unwrap(), [7]);
        assert_eq!(
            ids(None, Some((2, 8)), false).resolve("x", &have).unwrap(),
            [2, 5]
        );
        assert_eq!(ids(None, None, true).resolve("x", &have).unwrap(), have);
        assert!(ids(None, None, false).resolve("x", &have).is_err());
    }
}
