//! Deterministic, bounded world events. No wall clock, camera or renderer inputs.
//!
//! Owners aggregate their own deadlines into one next event. Dispatchers must
//! check entity generation and expected_version against authoritative state.
//! A popped event and its effects must be saved atomically with the whole world.

use crate::entity_pool::EntityId;

pub const MAX_EVENTS: usize = 256;
pub const MAX_DOMAINS: u8 = 16;
pub const MAX_SAME_TIME_DISPATCHES: u16 = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum Phase {
    Environment,
    Commit,
    Needs,
    Plan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Owner {
    Plant(EntityId),
    Tree(EntityId),
    Actor(EntityId),
    Job(EntityId),
    Domain(u8),
}

impl Owner {
    fn valid(self) -> bool {
        !matches!(self, Owner::Domain(id) if id >= MAX_DOMAINS)
    }
}

/// Field order is the documented total ordering; changing it changes saves.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EventKey {
    pub due_ms: u64,
    pub phase: Phase,
    pub owner: Owner,
    pub kind: u16,
    pub sequence: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledEvent {
    pub key: EventKey,
    pub expected_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventRequest {
    pub due_ms: u64,
    pub phase: Phase,
    pub owner: Owner,
    pub kind: u16,
    pub expected_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueError {
    Full,
    InvalidOwner,
    TimeWentBackwards,
    TimeExhausted,
    SequenceExhausted,
    SameTimeLimit,
}

/// Versioned save codecs should encode fields explicitly, then call restore.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueState<const N: usize> {
    pub now_ms: u64,
    pub next_sequence: u64,
    pub watermark: Option<EventKey>,
    pub same_time_count: u16,
    pub events: [Option<ScheduledEvent>; N],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoreError {
    Capacity,
    Sequence,
    Watermark,
    SameTimeCount,
    InvalidOwner,
    EventBeforeCursor,
    EventBeforeWatermark,
    DuplicateOwner,
    DuplicateSequence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventQueue<const N: usize = MAX_EVENTS> {
    state: QueueState<N>,
}

impl<const N: usize> EventQueue<N> {
    pub fn new(now_ms: u64) -> Self {
        assert!(N <= MAX_EVENTS, "event capacity exceeds world budget");
        Self {
            state: QueueState {
                now_ms,
                next_sequence: 1,
                watermark: None,
                same_time_count: 0,
                events: [None; N],
            },
        }
    }

    pub fn now_ms(&self) -> u64 {
        self.state.now_ms
    }
    pub fn len(&self) -> usize {
        self.state.events.iter().flatten().count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Replace an owner's next event, or insert it. The caller must supply the
    /// owner's minimum deadline, not call this separately for each subcondition.
    /// Errors leave the complete queue (including sequence) unchanged.
    pub fn schedule(&mut self, request: EventRequest) -> Result<ScheduledEvent, QueueError> {
        if !request.owner.valid() {
            return Err(QueueError::InvalidOwner);
        }
        if request.due_ms < self.state.now_ms {
            return Err(QueueError::TimeWentBackwards);
        }
        let index = self
            .state
            .events
            .iter()
            .position(|e| e.is_some_and(|e| e.key.owner == request.owner))
            .or_else(|| self.state.events.iter().position(Option::is_none))
            .ok_or(QueueError::Full)?;
        let next_sequence = self
            .state
            .next_sequence
            .checked_add(1)
            .ok_or(QueueError::SequenceExhausted)?;
        let mut key = EventKey {
            due_ms: request.due_ms,
            phase: request.phase,
            owner: request.owner,
            kind: request.kind,
            sequence: self.state.next_sequence,
        };
        if self.state.watermark.is_some_and(|last| key <= last) {
            key.due_ms = key.due_ms.checked_add(1).ok_or(QueueError::TimeExhausted)?;
        }
        let event = ScheduledEvent {
            key,
            expected_version: request.expected_version,
        };
        self.state.events[index] = Some(event);
        self.state.next_sequence = next_sequence;
        Ok(event)
    }

    pub fn cancel(&mut self, owner: Owner) -> Option<ScheduledEvent> {
        let index = self
            .state
            .events
            .iter()
            .position(|event| event.is_some_and(|e| e.key.owner == owner))?;
        self.state.events[index].take()
    }

    pub fn peek(&self) -> Option<ScheduledEvent> {
        self.state
            .events
            .iter()
            .flatten()
            .min_by_key(|event| event.key)
            .copied()
    }

    /// Return one event up to target; None proves the cursor reached target.
    /// Stopping after one event is a safe CPU-budget yield, provided the caller
    /// finishes its domain transaction before saving/yielding.
    /// SameTimeLimit is a fault: do not busy-retry or clear it on budget yields.
    pub fn pop_due(&mut self, target_ms: u64) -> Result<Option<ScheduledEvent>, QueueError> {
        if target_ms < self.state.now_ms {
            return Err(QueueError::TimeWentBackwards);
        }
        let Some(event) = self.peek().filter(|event| event.key.due_ms <= target_ms) else {
            self.state.now_ms = target_ms;
            return Ok(None);
        };
        let count = if self
            .state
            .watermark
            .is_some_and(|last| last.due_ms == event.key.due_ms)
        {
            self.state
                .same_time_count
                .checked_add(1)
                .ok_or(QueueError::SameTimeLimit)?
        } else {
            1
        };
        if count > MAX_SAME_TIME_DISPATCHES {
            return Err(QueueError::SameTimeLimit);
        }
        let removed = self.cancel(event.key.owner);
        debug_assert_eq!(removed, Some(event));
        self.state.now_ms = event.key.due_ms;
        self.state.watermark = Some(event.key);
        self.state.same_time_count = count;
        Ok(Some(event))
    }

    pub fn snapshot(&self) -> QueueState<N> {
        self.state.clone()
    }

    pub fn restore(state: QueueState<N>) -> Result<Self, RestoreError> {
        if N > MAX_EVENTS {
            return Err(RestoreError::Capacity);
        }
        if state.next_sequence == 0 {
            return Err(RestoreError::Sequence);
        }
        match state.watermark {
            Some(last) => {
                if !last.owner.valid()
                    || last.due_ms > state.now_ms
                    || last.sequence == 0
                    || last.sequence >= state.next_sequence
                {
                    return Err(RestoreError::Watermark);
                }
                if state.same_time_count == 0 || state.same_time_count > MAX_SAME_TIME_DISPATCHES {
                    return Err(RestoreError::SameTimeCount);
                }
            }
            None if state.same_time_count != 0 => return Err(RestoreError::SameTimeCount),
            None => (),
        }
        for (index, event) in state
            .events
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.map(|e| (i, e)))
        {
            if !event.key.owner.valid() {
                return Err(RestoreError::InvalidOwner);
            }
            if event.key.sequence == 0 || event.key.sequence >= state.next_sequence {
                return Err(RestoreError::Sequence);
            }
            if event.key.due_ms < state.now_ms {
                return Err(RestoreError::EventBeforeCursor);
            }
            if state.watermark.is_some_and(|last| event.key <= last) {
                return Err(RestoreError::EventBeforeWatermark);
            }
            if state
                .watermark
                .is_some_and(|last| event.key.sequence == last.sequence)
                || state.events[..index]
                    .iter()
                    .flatten()
                    .any(|e| e.key.sequence == event.key.sequence)
            {
                return Err(RestoreError::DuplicateSequence);
            }
            if state.events[..index]
                .iter()
                .flatten()
                .any(|e| e.key.owner == event.key.owner)
            {
                return Err(RestoreError::DuplicateOwner);
            }
        }
        Ok(Self { state })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(time: u64, phase: Phase, domain: u8) -> EventRequest {
        EventRequest {
            due_ms: time,
            phase,
            owner: Owner::Domain(domain),
            kind: 0,
            expected_version: 1,
        }
    }

    #[test]
    fn total_order_and_causal_backedge_are_independent_of_insertion_order() {
        let mut queue = EventQueue::<4>::new(0);
        queue.schedule(request(10, Phase::Plan, 0)).unwrap();
        queue.schedule(request(10, Phase::Environment, 1)).unwrap();
        queue.schedule(request(10, Phase::Commit, 2)).unwrap();
        assert_eq!(
            queue.pop_due(10).unwrap().unwrap().key.phase,
            Phase::Environment
        );
        assert_eq!(queue.pop_due(10).unwrap().unwrap().key.phase, Phase::Commit);
        let backedge = queue.schedule(request(10, Phase::Environment, 3)).unwrap();
        assert_eq!(backedge.key.due_ms, 11);
        assert_eq!(queue.pop_due(10).unwrap().unwrap().key.phase, Phase::Plan);
        assert_eq!(queue.pop_due(10), Ok(None));
        assert_eq!(queue.pop_due(11), Ok(Some(backedge)));
    }

    #[test]
    fn same_phase_owner_backedge_is_deferred_even_with_a_new_sequence() {
        let mut queue = EventQueue::<2>::new(0);
        queue.schedule(request(2, Phase::Plan, 9)).unwrap();
        queue.pop_due(2).unwrap();
        assert_eq!(
            queue
                .schedule(request(2, Phase::Plan, 1))
                .unwrap()
                .key
                .due_ms,
            3
        );
    }

    #[test]
    fn capacity_and_exhaustion_fail_atomically_but_owner_replacement_fits() {
        let mut queue = EventQueue::<1>::new(3);
        queue.schedule(request(4, Phase::Needs, 0)).unwrap();
        let before = queue.snapshot();
        assert_eq!(
            queue.schedule(request(4, Phase::Plan, 1)),
            Err(QueueError::Full)
        );
        assert_eq!(queue.snapshot(), before);
        assert_eq!(
            queue.schedule(request(2, Phase::Plan, 0)),
            Err(QueueError::TimeWentBackwards)
        );
        assert_eq!(
            queue.schedule(request(4, Phase::Plan, 16)),
            Err(QueueError::InvalidOwner)
        );
        queue.schedule(request(5, Phase::Needs, 0)).unwrap();
        assert_eq!(queue.len(), 1);
        let mut state = queue.snapshot();
        state.next_sequence = u64::MAX;
        let mut queue = EventQueue::restore(state).unwrap();
        let before = queue.snapshot();
        assert_eq!(
            queue.schedule(request(5, Phase::Needs, 0)),
            Err(QueueError::SequenceExhausted)
        );
        assert_eq!(queue.snapshot(), before);
    }

    #[test]
    fn time_exhaustion_and_idle_cursor_do_not_allow_rewinding() {
        let mut queue = EventQueue::<2>::new(u64::MAX);
        queue.schedule(request(u64::MAX, Phase::Plan, 2)).unwrap();
        queue.pop_due(u64::MAX).unwrap();
        let before = queue.snapshot();
        assert_eq!(
            queue.schedule(request(u64::MAX, Phase::Environment, 0)),
            Err(QueueError::TimeExhausted)
        );
        assert_eq!(queue.snapshot(), before);
        let mut empty = EventQueue::<2>::new(0);
        assert_eq!(empty.pop_due(500), Ok(None));
        assert_eq!(
            empty.schedule(request(499, Phase::Plan, 0)),
            Err(QueueError::TimeWentBackwards)
        );
        assert_eq!(empty.pop_due(499), Err(QueueError::TimeWentBackwards));
    }

    fn replay(chunk: u64, reload: bool) -> (Vec<EventKey>, QueueState<4>) {
        let mut queue = EventQueue::<4>::new(0);
        queue.schedule(request(0, Phase::Plan, 0)).unwrap();
        let mut trace = Vec::new();
        for target in (0..=100).step_by(chunk as usize) {
            while let Some(event) = queue.pop_due(target).unwrap() {
                trace.push(event.key);
                if event.key.phase == Phase::Plan {
                    queue
                        .schedule(request(event.key.due_ms, Phase::Environment, 1))
                        .unwrap();
                } else if event.key.due_ms < 90 {
                    queue
                        .schedule(request(event.key.due_ms + 9, Phase::Plan, 0))
                        .unwrap();
                }
                if reload {
                    queue = EventQueue::restore(queue.snapshot()).unwrap();
                }
            }
        }
        (trace, queue.snapshot())
    }

    #[test]
    fn sliced_offline_and_checkpoint_replays_match() {
        assert_eq!(replay(100, false), replay(1, true));
        assert_eq!(replay(10, true), replay(1, false));
    }

    #[test]
    fn reload_cannot_erase_zero_time_loop_limit() {
        let mut queue = EventQueue::<1>::new(0);
        for _ in 0..MAX_SAME_TIME_DISPATCHES {
            queue.schedule(request(0, Phase::Plan, 0)).unwrap();
            queue.pop_due(0).unwrap().unwrap();
            queue = EventQueue::restore(queue.snapshot()).unwrap();
        }
        queue.schedule(request(0, Phase::Plan, 0)).unwrap();
        let before = queue.snapshot();
        assert_eq!(queue.pop_due(0), Err(QueueError::SameTimeLimit));
        assert_eq!(queue.snapshot(), before);
    }

    #[test]
    fn restore_rejects_corrupt_ownership_order_and_counters() {
        let mut queue = EventQueue::<3>::new(0);
        queue.schedule(request(2, Phase::Plan, 0)).unwrap();
        let good = queue.snapshot();
        let mut bad = good.clone();
        bad.events[1] = bad.events[0];
        assert_eq!(
            EventQueue::restore(bad),
            Err(RestoreError::DuplicateSequence)
        );
        let mut bad = good.clone();
        bad.events[1] = bad.events[0];
        bad.events[1].as_mut().unwrap().key.sequence = 2;
        bad.next_sequence = 3;
        assert_eq!(EventQueue::restore(bad), Err(RestoreError::DuplicateOwner));
        let mut bad = good.clone();
        bad.now_ms = 3;
        assert_eq!(
            EventQueue::restore(bad),
            Err(RestoreError::EventBeforeCursor)
        );
        let mut bad = good;
        bad.same_time_count = 1;
        assert_eq!(EventQueue::restore(bad), Err(RestoreError::SameTimeCount));
    }
}
