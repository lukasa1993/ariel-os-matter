//! Portable commissioning, subscription, and event state.

use heapless::{Deque, Vec};

use crate::access::FabricIndex;
use crate::{Error, Result};

/// Maximum encoded event payload retained by the portable event log.
pub const MAX_EVENT_PAYLOAD: usize = 192;

/// Commissioning lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommissioningState {
    /// The node has no fabric and no open window.
    Uncommissioned,
    /// A commissioning window is open.
    WindowOpen {
        /// Monotonic expiry time in milliseconds.
        expires_at_ms: u64,
        /// Enhanced window indicator.
        enhanced: bool,
    },
    /// The fail-safe is armed during commissioning.
    FailSafeArmed {
        /// Monotonic expiry time in milliseconds.
        expires_at_ms: u64,
        /// Fabric that owns the fail-safe, when assigned.
        fabric: Option<FabricIndex>,
    },
    /// The node has at least one commissioned fabric.
    Commissioned,
}

/// Fail-closed commissioning state machine.
pub struct CommissioningManager {
    state: CommissioningState,
    fabric_count: u8,
}

impl CommissioningManager {
    /// Create an uncommissioned manager.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: CommissioningState::Uncommissioned,
            fabric_count: 0,
        }
    }

    /// Return the current state.
    #[must_use]
    pub const fn state(&self) -> CommissioningState {
        self.state
    }

    /// Open a commissioning window.
    pub fn open_window(&mut self, now_ms: u64, timeout_ms: u64, enhanced: bool) -> Result<()> {
        if timeout_ms == 0 || matches!(self.state, CommissioningState::FailSafeArmed { .. }) {
            return Err(Error::InvalidState);
        }
        self.state = CommissioningState::WindowOpen {
            expires_at_ms: now_ms
                .checked_add(timeout_ms)
                .ok_or(Error::InvalidArgument)?,
            enhanced,
        };
        Ok(())
    }

    /// Arm the commissioning fail-safe.
    pub fn arm_fail_safe(
        &mut self,
        now_ms: u64,
        timeout_ms: u64,
        fabric: Option<FabricIndex>,
    ) -> Result<()> {
        if timeout_ms == 0 || !matches!(self.state, CommissioningState::WindowOpen { .. }) {
            return Err(Error::InvalidState);
        }
        self.state = CommissioningState::FailSafeArmed {
            expires_at_ms: now_ms
                .checked_add(timeout_ms)
                .ok_or(Error::InvalidArgument)?,
            fabric,
        };
        Ok(())
    }

    /// Commit a commissioned fabric.
    pub fn complete(&mut self) -> Result<()> {
        if !matches!(self.state, CommissioningState::FailSafeArmed { .. }) {
            return Err(Error::InvalidState);
        }
        self.fabric_count = self.fabric_count.checked_add(1).ok_or(Error::Capacity)?;
        self.state = CommissioningState::Commissioned;
        Ok(())
    }

    /// Remove a fabric and update the commissioning state.
    pub fn fabric_removed(&mut self) -> Result<()> {
        self.fabric_count = self
            .fabric_count
            .checked_sub(1)
            .ok_or(Error::InvalidState)?;
        self.state = if self.fabric_count == 0 {
            CommissioningState::Uncommissioned
        } else {
            CommissioningState::Commissioned
        };
        Ok(())
    }

    /// Process a monotonic clock tick and close or roll back expired state.
    ///
    /// Returns `true` when the fail-safe expired and persistent commissioning
    /// data must be restored from its committed snapshot.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        match self.state {
            CommissioningState::WindowOpen { expires_at_ms, .. } if now_ms >= expires_at_ms => {
                self.state = if self.fabric_count == 0 {
                    CommissioningState::Uncommissioned
                } else {
                    CommissioningState::Commissioned
                };
                false
            }
            CommissioningState::FailSafeArmed { expires_at_ms, .. } if now_ms >= expires_at_ms => {
                self.state = if self.fabric_count == 0 {
                    CommissioningState::Uncommissioned
                } else {
                    CommissioningState::Commissioned
                };
                true
            }
            CommissioningState::Uncommissioned
            | CommissioningState::WindowOpen { .. }
            | CommissioningState::FailSafeArmed { .. }
            | CommissioningState::Commissioned => false,
        }
    }
}

impl Default for CommissioningManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Matter event priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventPriority {
    /// Informational event.
    Info,
    /// Diagnostic event.
    Debug,
    /// Critical event.
    Critical,
}

/// One retained event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventRecord {
    /// Monotonic event number.
    pub number: u64,
    /// Timestamp in milliseconds from the selected Matter time source.
    pub timestamp_ms: u64,
    /// Endpoint identifier.
    pub endpoint: u16,
    /// Cluster identifier.
    pub cluster: u32,
    /// Event identifier.
    pub event: u32,
    /// Priority.
    pub priority: EventPriority,
    /// Typed adapter payload. The application does not access Matter TLV.
    pub payload: Vec<u8, MAX_EVENT_PAYLOAD>,
}

/// Fixed-capacity event log. Oldest records are removed only after capacity is
/// reached, while event numbers remain monotonic.
pub struct EventLog<const N: usize> {
    records: Deque<EventRecord, N>,
    next_number: u64,
}

impl<const N: usize> EventLog<N> {
    /// Create an event log with the next persisted event number.
    #[must_use]
    pub const fn new(next_number: u64) -> Self {
        Self {
            records: Deque::new(),
            next_number,
        }
    }

    /// Emit an event and return its assigned event number.
    pub fn emit(
        &mut self,
        timestamp_ms: u64,
        endpoint: u16,
        cluster: u32,
        event: u32,
        priority: EventPriority,
        payload: &[u8],
    ) -> Result<u64> {
        let mut stored_payload = Vec::new();
        stored_payload
            .extend_from_slice(payload)
            .map_err(|_| Error::Capacity)?;
        let number = self.next_number;
        self.next_number = self.next_number.checked_add(1).ok_or(Error::Capacity)?;
        if self.records.is_full() {
            let _removed = self.records.pop_front().ok_or(Error::InvalidState)?;
        }
        self.records
            .push_back(EventRecord {
                number,
                timestamp_ms,
                endpoint,
                cluster,
                event,
                priority,
                payload: stored_payload,
            })
            .map_err(|_| Error::Capacity)?;
        Ok(number)
    }

    /// Iterate over events from a given event number.
    pub fn from(&self, number: u64) -> impl Iterator<Item = &EventRecord> {
        self.records
            .iter()
            .filter(move |record| record.number >= number)
    }

    /// Return the next number that must be persisted as the event epoch.
    #[must_use]
    pub const fn next_number(&self) -> u64 {
        self.next_number
    }
}

/// One active Matter subscription.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Subscription {
    /// Owning fabric.
    pub fabric: FabricIndex,
    /// Subscriber node identifier.
    pub peer_node_id: u64,
    /// Subscription identifier.
    pub id: u32,
    /// Minimum report interval.
    pub min_interval_ms: u64,
    /// Maximum report interval.
    pub max_interval_ms: u64,
    /// Time of the last completed report.
    pub last_report_ms: u64,
    /// Pending change flag.
    pub dirty: bool,
}

impl Subscription {
    /// Return true when a report is due.
    #[must_use]
    pub fn due(&self, now_ms: u64) -> bool {
        let elapsed = now_ms.saturating_sub(self.last_report_ms);
        elapsed >= self.max_interval_ms || (self.dirty && elapsed >= self.min_interval_ms)
    }
}

/// Fixed-capacity subscription table.
pub struct SubscriptionTable<const N: usize> {
    entries: Vec<Subscription, N>,
}

impl<const N: usize> SubscriptionTable<N> {
    /// Create an empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a subscription.
    pub fn add(&mut self, subscription: Subscription) -> Result<()> {
        if subscription.max_interval_ms < subscription.min_interval_ms
            || subscription.max_interval_ms == 0
        {
            return Err(Error::InvalidArgument);
        }
        if self.entries.iter().any(|entry| {
            entry.fabric == subscription.fabric
                && entry.peer_node_id == subscription.peer_node_id
                && entry.id == subscription.id
        }) {
            return Err(Error::Conflict);
        }
        self.entries.push(subscription).map_err(|_| Error::Capacity)
    }

    /// Mark every subscription as dirty after a data-model change.
    pub fn notify_all(&mut self) {
        for entry in &mut self.entries {
            entry.dirty = true;
        }
    }

    /// Mark a completed report.
    pub fn reported(
        &mut self,
        fabric: FabricIndex,
        peer_node_id: u64,
        id: u32,
        now_ms: u64,
    ) -> Result<()> {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| {
                entry.fabric == fabric && entry.peer_node_id == peer_node_id && entry.id == id
            })
            .ok_or(Error::NotFound)?;
        entry.last_report_ms = now_ms;
        entry.dirty = false;
        Ok(())
    }

    /// Remove all subscriptions for a fabric.
    pub fn remove_fabric(&mut self, fabric: FabricIndex) {
        self.entries.retain(|entry| entry.fabric != fabric);
    }

    /// Iterate over subscriptions that need a report.
    pub fn due(&self, now_ms: u64) -> impl Iterator<Item = &Subscription> {
        self.entries.iter().filter(move |entry| entry.due(now_ms))
    }
}

impl<const N: usize> Default for SubscriptionTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fabric() -> FabricIndex {
        match FabricIndex::new(1) {
            Ok(value) => value,
            Err(error) => std::panic::panic_any(error),
        }
    }

    #[test]
    fn fail_safe_expiry_requests_rollback() {
        let mut state = CommissioningManager::new();
        assert_eq!(state.open_window(100, 1_000, false), Ok(()));
        assert_eq!(state.arm_fail_safe(200, 100, None), Ok(()));
        assert!(state.tick(300));
        assert_eq!(state.state(), CommissioningState::Uncommissioned);
    }

    #[test]
    fn event_log_keeps_monotonic_numbers_after_eviction() {
        let mut events = EventLog::<2>::new(40);
        assert_eq!(events.emit(1, 1, 6, 0, EventPriority::Info, b"a"), Ok(40));
        assert_eq!(events.emit(2, 1, 6, 0, EventPriority::Info, b"b"), Ok(41));
        assert_eq!(events.emit(3, 1, 6, 0, EventPriority::Info, b"c"), Ok(42));
        assert_eq!(
            events
                .from(0)
                .map(|event| event.number)
                .collect::<std::vec::Vec<_>>(),
            [41, 42]
        );
    }

    #[test]
    fn dirty_subscription_honors_minimum_interval() {
        let mut table = SubscriptionTable::<2>::new();
        assert_eq!(
            table.add(Subscription {
                fabric: fabric(),
                peer_node_id: 7,
                id: 9,
                min_interval_ms: 100,
                max_interval_ms: 1_000,
                last_report_ms: 0,
                dirty: false,
            }),
            Ok(())
        );
        table.notify_all();
        assert_eq!(table.due(99).count(), 0);
        assert_eq!(table.due(100).count(), 1);
    }
}
