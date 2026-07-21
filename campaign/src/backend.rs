//! Storage backend trait for host-agnostic testing.
//!
//! Provides a [`StorageBackend`] trait that abstracts over Soroban's persistent
//! and temporary storage.  The [`SorobanStorage`] implementation delegates to
//! `Env::storage()`.  The [`InMemoryStorage`] implementation (behind the
//! `test-backend` feature flag) uses an in-memory `BTreeMap` for testing
//! without deploying contracts to the Soroban host.

extern crate alloc;

use crate::types::DataKey;
use soroban_sdk::{Env, TryFromVal, Val};

// ─── Trait ───────────────────────────────────────────────────────────────────

/// Abstract storage backend for Soroban-compatible key/value storage.
///
/// Split into persistent and temporary sections mirroring Soroban's own
/// storage model.  TTL operations are no-ops for backends that do not
/// support ledger-based expiry (e.g. [`InMemoryStorage`]).
pub trait StorageBackend {
    // ── Persistent ──────────────────────────────────────────────────────────

    fn persistent_has(&self, env: &Env, key: &DataKey) -> bool;

    fn persistent_get<T: TryFromVal<Env, Val> + Clone + 'static>(
        &self,
        env: &Env,
        key: &DataKey,
    ) -> Option<T>;

    fn persistent_set<T: soroban_sdk::IntoVal<Env, Val> + Clone + 'static>(
        &self,
        env: &Env,
        key: &DataKey,
        value: &T,
    );

    fn persistent_remove(&self, env: &Env, key: &DataKey);

    fn persistent_extend_ttl(&self, env: &Env, key: &DataKey, threshold: u32, bump: u32);

    // ── Temporary ───────────────────────────────────────────────────────────

    fn temporary_has(&self, env: &Env, key: &DataKey) -> bool;

    fn temporary_get<T: TryFromVal<Env, Val> + Clone + 'static>(
        &self,
        env: &Env,
        key: &DataKey,
    ) -> Option<T>;

    fn temporary_set<T: soroban_sdk::IntoVal<Env, Val> + Clone + 'static>(
        &self,
        env: &Env,
        key: &DataKey,
        value: &T,
    );

    fn temporary_remove(&self, env: &Env, key: &DataKey);

    fn temporary_extend_ttl(&self, env: &Env, key: &DataKey, threshold: u32, bump: u32);
}

// ─── Soroban host backend ────────────────────────────────────────────────────

/// Default backend that delegates to `Env::storage()`.
pub struct SorobanStorage;

impl StorageBackend for SorobanStorage {
    fn persistent_has(&self, env: &Env, key: &DataKey) -> bool {
        env.storage().persistent().has(key)
    }

    fn persistent_get<T: TryFromVal<Env, Val> + Clone + 'static>(
        &self,
        env: &Env,
        key: &DataKey,
    ) -> Option<T> {
        env.storage().persistent().get(key)
    }

    fn persistent_set<T: soroban_sdk::IntoVal<Env, Val> + Clone + 'static>(
        &self,
        env: &Env,
        key: &DataKey,
        value: &T,
    ) {
        env.storage().persistent().set(key, value);
    }

    fn persistent_remove(&self, env: &Env, key: &DataKey) {
        env.storage().persistent().remove(key);
    }

    fn persistent_extend_ttl(&self, env: &Env, key: &DataKey, threshold: u32, bump: u32) {
        env.storage().persistent().extend_ttl(key, threshold, bump);
    }

    fn temporary_has(&self, env: &Env, key: &DataKey) -> bool {
        env.storage().temporary().has(key)
    }

    fn temporary_get<T: TryFromVal<Env, Val> + Clone + 'static>(
        &self,
        env: &Env,
        key: &DataKey,
    ) -> Option<T> {
        env.storage().temporary().get(key)
    }

    fn temporary_set<T: soroban_sdk::IntoVal<Env, Val> + Clone + 'static>(
        &self,
        env: &Env,
        key: &DataKey,
        value: &T,
    ) {
        env.storage().temporary().set(key, value);
    }

    fn temporary_remove(&self, env: &Env, key: &DataKey) {
        env.storage().temporary().remove(key);
    }

    fn temporary_extend_ttl(&self, env: &Env, key: &DataKey, threshold: u32, bump: u32) {
        env.storage().temporary().extend_ttl(key, threshold, bump);
    }
}

// ─── In-memory backend (test-only) ───────────────────────────────────────────

/// In-memory storage backend for host-agnostic testing.
///
/// Uses `BTreeMap<String, Box<dyn Any>>` with a deterministic string key
/// derived from `DataKey`'s `Debug` output.  No Soroban host or contract
/// deployment required.
///
/// TTL operations are no-ops — in-memory storage does not expire.
#[cfg(all(feature = "test-backend", test))]
pub struct InMemoryStorage {
    persistent: core::cell::RefCell<
        alloc::collections::BTreeMap<alloc::string::String, alloc::boxed::Box<dyn core::any::Any>>,
    >,
    temporary: core::cell::RefCell<
        alloc::collections::BTreeMap<alloc::string::String, alloc::boxed::Box<dyn core::any::Any>>,
    >,
}

#[cfg(all(feature = "test-backend", test))]
impl InMemoryStorage {
    /// Create a new empty in-memory storage instance.
    pub fn new() -> Self {
        Self {
            persistent: core::cell::RefCell::new(alloc::collections::BTreeMap::new()),
            temporary: core::cell::RefCell::new(alloc::collections::BTreeMap::new()),
        }
    }

    fn key_string(key: &DataKey) -> alloc::string::String {
        alloc::format!("{:?}", key)
    }
}

#[cfg(all(feature = "test-backend", test))]
impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "test-backend", test))]
impl StorageBackend for InMemoryStorage {
    fn persistent_has(&self, _env: &Env, key: &DataKey) -> bool {
        self.persistent
            .borrow()
            .contains_key(&Self::key_string(key))
    }

    fn persistent_get<T: TryFromVal<Env, Val> + Clone + 'static>(
        &self,
        _env: &Env,
        key: &DataKey,
    ) -> Option<T> {
        let map = self.persistent.borrow();
        let boxed = map.get(&Self::key_string(key))?;
        let val_ref = boxed.downcast_ref::<T>()?;
        Some(T::clone(val_ref))
    }

    fn persistent_set<T: soroban_sdk::IntoVal<Env, Val> + Clone + 'static>(
        &self,
        _env: &Env,
        key: &DataKey,
        value: &T,
    ) {
        self.persistent
            .borrow_mut()
            .insert(Self::key_string(key), alloc::boxed::Box::new(value.clone()));
    }

    fn persistent_remove(&self, _env: &Env, key: &DataKey) {
        self.persistent.borrow_mut().remove(&Self::key_string(key));
    }

    fn persistent_extend_ttl(&self, _env: &Env, _key: &DataKey, _threshold: u32, _bump: u32) {
        // No-op
    }

    fn temporary_has(&self, _env: &Env, key: &DataKey) -> bool {
        self.temporary.borrow().contains_key(&Self::key_string(key))
    }

    fn temporary_get<T: TryFromVal<Env, Val> + Clone + 'static>(
        &self,
        _env: &Env,
        key: &DataKey,
    ) -> Option<T> {
        let map = self.temporary.borrow();
        let boxed = map.get(&Self::key_string(key))?;
        let val_ref = boxed.downcast_ref::<T>()?;
        Some(T::clone(val_ref))
    }

    fn temporary_set<T: soroban_sdk::IntoVal<Env, Val> + Clone + 'static>(
        &self,
        _env: &Env,
        key: &DataKey,
        value: &T,
    ) {
        self.temporary
            .borrow_mut()
            .insert(Self::key_string(key), alloc::boxed::Box::new(value.clone()));
    }

    fn temporary_remove(&self, _env: &Env, key: &DataKey) {
        self.temporary.borrow_mut().remove(&Self::key_string(key));
    }

    fn temporary_extend_ttl(&self, _env: &Env, _key: &DataKey, _threshold: u32, _bump: u32) {
        // No-op
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(all(test, feature = "test-backend"))]
mod tests {
    use super::*;
    use crate::types::*;
    use soroban_sdk::Env;

    #[test]
    fn in_memory_persistent_set_get_i128() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        assert!(!storage.persistent_has(&env, &DataKey::TotalRaised));

        storage.persistent_set(&env, &DataKey::TotalRaised, &42i128);
        assert!(storage.persistent_has(&env, &DataKey::TotalRaised));

        let value: Option<i128> = storage.persistent_get(&env, &DataKey::TotalRaised);
        assert_eq!(value, Some(42));
    }

    #[test]
    fn in_memory_persistent_set_get_bool() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.persistent_set(&env, &DataKey::Frozen, &true);
        let value: Option<bool> = storage.persistent_get(&env, &DataKey::Frozen);
        assert_eq!(value, Some(true));
    }

    #[test]
    fn in_memory_persistent_set_get_u64() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.persistent_set(&env, &DataKey::DonationCount, &100u64);
        let value: Option<u64> = storage.persistent_get(&env, &DataKey::DonationCount);
        assert_eq!(value, Some(100));
    }

    #[test]
    fn in_memory_persistent_set_get_u32() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.persistent_set(&env, &DataKey::UniqueDonorCount, &7u32);
        let value: Option<u32> = storage.persistent_get(&env, &DataKey::UniqueDonorCount);
        assert_eq!(value, Some(7));
    }

    #[test]
    fn in_memory_persistent_remove() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.persistent_set(&env, &DataKey::TotalRaised, &1000i128);
        assert!(storage.persistent_has(&env, &DataKey::TotalRaised));

        storage.persistent_remove(&env, &DataKey::TotalRaised);
        assert!(!storage.persistent_has(&env, &DataKey::TotalRaised));

        let value: Option<i128> = storage.persistent_get(&env, &DataKey::TotalRaised);
        assert_eq!(value, None);
    }

    #[test]
    fn in_memory_persistent_overwrite() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.persistent_set(&env, &DataKey::TotalRaised, &100i128);
        assert_eq!(
            storage.persistent_get::<i128>(&env, &DataKey::TotalRaised),
            Some(100)
        );

        storage.persistent_set(&env, &DataKey::TotalRaised, &200i128);
        assert_eq!(
            storage.persistent_get::<i128>(&env, &DataKey::TotalRaised),
            Some(200)
        );
    }

    #[test]
    fn in_memory_temporary_set_get() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        assert!(!storage.temporary_has(&env, &DataKey::ContractStatus));

        storage.temporary_set(&env, &DataKey::ContractStatus, &1u32);
        assert!(storage.temporary_has(&env, &DataKey::ContractStatus));

        let value: Option<u32> = storage.temporary_get(&env, &DataKey::ContractStatus);
        assert_eq!(value, Some(1));
    }

    #[test]
    fn in_memory_temporary_remove() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.temporary_set(&env, &DataKey::ContractStatus, &5u32);
        storage.temporary_remove(&env, &DataKey::ContractStatus);
        assert!(!storage.temporary_has(&env, &DataKey::ContractStatus));
    }

    #[test]
    fn in_memory_temporary_overwrite() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.temporary_set(&env, &DataKey::ContractStatus, &1u32);
        storage.temporary_set(&env, &DataKey::ContractStatus, &2u32);

        let value: Option<u32> = storage.temporary_get(&env, &DataKey::ContractStatus);
        assert_eq!(value, Some(2));
    }

    #[test]
    fn in_memory_extend_ttl_no_op() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        // Should not panic
        storage.persistent_extend_ttl(&env, &DataKey::TotalRaised, 100, 200);
        storage.temporary_extend_ttl(&env, &DataKey::ContractStatus, 100, 200);
    }

    #[test]
    fn in_memory_multiple_keys_isolated() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.persistent_set(&env, &DataKey::TotalRaised, &5000i128);
        storage.persistent_set(&env, &DataKey::DonationCount, &42u64);
        storage.persistent_set(&env, &DataKey::UniqueDonorCount, &10u32);
        storage.persistent_set(&env, &DataKey::ReleaseCount, &3u64);

        assert_eq!(
            storage.persistent_get::<i128>(&env, &DataKey::TotalRaised),
            Some(5000)
        );
        assert_eq!(
            storage.persistent_get::<u64>(&env, &DataKey::DonationCount),
            Some(42)
        );
        assert_eq!(
            storage.persistent_get::<u32>(&env, &DataKey::UniqueDonorCount),
            Some(10)
        );
        assert_eq!(
            storage.persistent_get::<u64>(&env, &DataKey::ReleaseCount),
            Some(3)
        );
    }

    #[test]
    fn in_memory_persistent_and_temporary_independent() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.persistent_set(&env, &DataKey::TotalRaised, &100i128);
        storage.temporary_set(&env, &DataKey::ContractStatus, &1u32);

        storage.persistent_remove(&env, &DataKey::TotalRaised);
        assert!(!storage.persistent_has(&env, &DataKey::TotalRaised));
        assert!(storage.temporary_has(&env, &DataKey::ContractStatus));
    }

    #[test]
    fn in_memory_missing_key_returns_none() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        assert_eq!(
            storage.persistent_get::<i128>(&env, &DataKey::TotalRaised),
            None
        );
        assert_eq!(
            storage.temporary_get::<u32>(&env, &DataKey::ContractStatus),
            None
        );
    }

    #[test]
    fn in_memory_reentrancy_lock() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        assert!(!storage.temporary_has(&env, &DataKey::ReentrancyLock));
        storage.temporary_set(&env, &DataKey::ReentrancyLock, &true);
        assert!(storage.temporary_has(&env, &DataKey::ReentrancyLock));

        storage.temporary_remove(&env, &DataKey::ReentrancyLock);
        assert!(!storage.temporary_has(&env, &DataKey::ReentrancyLock));
    }

    #[test]
    fn in_memory_milestone_index_key() {
        let env = Env::default();
        let storage = InMemoryStorage::new();

        storage.persistent_set(&env, &DataKey::MilestoneData(0), &42i128);
        storage.persistent_set(&env, &DataKey::MilestoneData(1), &99i128);

        assert_eq!(
            storage.persistent_get::<i128>(&env, &DataKey::MilestoneData(0)),
            Some(42)
        );
        assert_eq!(
            storage.persistent_get::<i128>(&env, &DataKey::MilestoneData(1)),
            Some(99)
        );

        storage.persistent_remove(&env, &DataKey::MilestoneData(0));
        assert!(!storage.persistent_has(&env, &DataKey::MilestoneData(0)));
        assert!(storage.persistent_has(&env, &DataKey::MilestoneData(1)));
    }

    #[test]
    fn in_memory_isolated_instances() {
        let env = Env::default();
        let storage1 = InMemoryStorage::new();
        let storage2 = InMemoryStorage::new();

        storage1.persistent_set(&env, &DataKey::TotalRaised, &100i128);
        assert!(storage1.persistent_has(&env, &DataKey::TotalRaised));
        assert!(!storage2.persistent_has(&env, &DataKey::TotalRaised));
    }
}
