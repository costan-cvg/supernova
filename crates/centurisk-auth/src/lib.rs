//! Authentication, authorization, and tenant isolation.
//!
//! Key types:
//! - `PolicyGate` trait: called on every data path from Inc 1 onward
//! - `AllowAllPolicy`: permissive stub, logs every decision (replaced by Cedar in Inc 5)
//! - `TenantContext`: required on every repository operation for pool/member isolation

pub mod policy;
pub mod tenant;
pub mod principal;

pub use policy::{AllowAllPolicy, AuthzDecision, PolicyGate};
pub use principal::Principal;
pub use tenant::TenantContext;
