//! Newtype IDs for domain entities. All UUID v7 (time-ordered for index locality).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_id!(AssetId, "Unique identifier for an asset.");
define_id!(PoolId, "Unique identifier for a risk pool.");
define_id!(MemberId, "Unique identifier for a pool member.");
define_id!(ActorId, "Unique identifier for a user/actor performing an action.");
define_id!(MutationId, "Unique identifier for a field mutation.");
define_id!(GrantId, "Unique identifier for an access grant.");
define_id!(RuleId, "Unique identifier for a quality/accuracy rule.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let a = AssetId::new();
        let b = AssetId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn serde_roundtrip() {
        let id = PoolId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: PoolId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn display_is_uuid() {
        let id = MemberId::new();
        let s = id.to_string();
        // UUID v7 is valid UUID format
        assert!(Uuid::parse_str(&s).is_ok());
    }
}
