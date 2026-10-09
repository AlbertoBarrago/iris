//! Which standard protects a message: OpenPGP or S/MIME.

use serde::{Deserialize, Serialize};

/// Which standard a message goes out under.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Standard {
    #[default]
    Pgp,
    Smime,
}
