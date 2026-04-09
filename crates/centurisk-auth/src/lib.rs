//! Authentication, authorization, and tenant isolation.
//!
//! Key types:
//! - `PolicyGate` trait: called on every data path from Inc 1 onward
//! - `CedarPolicyGate`: Cedar ABAC engine (Inc 5+)
//! - `AllowAllPolicy`: permissive stub for testing
//! - `TenantContext`: required on every repository operation for pool/member isolation

pub mod cedar;
pub mod policy;
pub mod tenant;
pub mod principal;

pub use cedar::CedarPolicyGate;
pub use policy::{AllowAllPolicy, AuthzDecision, PolicyGate};
pub use principal::Principal;
pub use tenant::TenantContext;
