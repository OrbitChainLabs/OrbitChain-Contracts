//! Role-Based Access Control (RBAC) System
//!
//! Provides a secure and unified way to manage administrative roles and verify permissions
//! using Soroban's native `require_auth()` mechanism.

use soroban_sdk::{contracttype, Address, Env};

/// Storage keys for RBAC
#[contracttype]
pub enum RbacStorageKey {
    /// The global administrator address
    CoreAdmin,
}

/// Helper functions for managing account roles
pub struct Rbac;

impl Rbac {
    /// Get the current administrator address from storage
    pub fn get_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&RbacStorageKey::CoreAdmin)
    }

    /// Set a new administrator address (used during initialization)
    pub fn set_admin(env: &Env, admin: &Address) {
        env.storage().instance().set(&RbacStorageKey::CoreAdmin, admin);
    }

    /// Check if an administrator is set
    pub fn has_admin(env: &Env) -> bool {
        env.storage().instance().has(&RbacStorageKey::CoreAdmin)
    }

    /// Verify that the stored admin has authorized the current operation.
    /// Panics if the admin is not set or authorization fails.
    pub fn require_admin(env: &Env) {
        if let Some(admin) = Self::get_admin(env) {
            admin.require_auth();
        } else {
            panic!("Admin not initialized");
        }
    }

    /// Verify that a specific address is the admin and has authorized the operation.
    pub fn require_admin_auth(env: &Env, caller: &Address) {
        if let Some(admin) = Self::get_admin(env) {
            if caller == &admin {
                caller.require_auth();
            } else {
                panic!("Unauthorized: caller is not admin");
            }
        } else {
            panic!("Admin not initialized");
        }
    }

    /// Update the administrator address (admin only)
    pub fn update_admin(env: &Env, caller: &Address, new_admin: &Address) {
        Self::require_admin_auth(env, caller);
        Self::set_admin(env, new_admin);
    }
}
