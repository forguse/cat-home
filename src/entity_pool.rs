//! Fixed-capacity handles that cannot silently point at a reused entity slot.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EntityId {
    index: u16,
    generation: u32,
}

impl EntityId {
    pub const fn from_parts(index: u16, generation: u32) -> Option<Self> {
        if generation == 0 {
            None
        } else {
            Some(Self { index, generation })
        }
    }

    pub const fn index(self) -> u16 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// Save-facing data, validated on restore; do not serialize raw struct bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotState<T> {
    pub generation: u32,
    pub retired: bool,
    pub value: Option<T>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoreError {
    TooManySlots,
    InvalidGeneration(usize),
    InvalidRetiredSlot(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityPool<T, const N: usize> {
    slots: [SlotState<T>; N],
    len: usize,
}

impl<T, const N: usize> Default for EntityPool<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> EntityPool<T, N> {
    pub fn new() -> Self {
        assert!(
            N <= usize::from(u16::MAX) + 1,
            "entity pool exceeds ID range"
        );
        Self {
            slots: std::array::from_fn(|_| SlotState {
                generation: 1,
                retired: false,
                value: None,
            }),
            len: 0,
        }
    }

    /// Return the original value if full, so a failed reservation loses nothing.
    pub fn insert(&mut self, value: T) -> Result<EntityId, T> {
        let Some(index) = self
            .slots
            .iter()
            .position(|s| !s.retired && s.value.is_none())
        else {
            return Err(value);
        };
        let slot = &mut self.slots[index];
        slot.value = Some(value);
        self.len += 1;
        Ok(EntityId {
            index: index as u16,
            generation: slot.generation,
        })
    }

    pub fn get(&self, id: EntityId) -> Option<&T> {
        let slot = self.slots.get(usize::from(id.index))?;
        if !slot.retired && slot.generation == id.generation {
            slot.value.as_ref()
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut T> {
        let slot = self.slots.get_mut(usize::from(id.index))?;
        if !slot.retired && slot.generation == id.generation {
            slot.value.as_mut()
        } else {
            None
        }
    }

    pub fn remove(&mut self, id: EntityId) -> Option<T> {
        let slot = self.slots.get_mut(usize::from(id.index))?;
        if slot.retired || slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take()?;
        self.len -= 1;
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
        } else {
            // Never wrap generation back to a previously issued handle.
            slot.retired = true;
        }
        Some(value)
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.value.as_ref().map(|value| {
                (
                    EntityId {
                        index: index as u16,
                        generation: slot.generation,
                    },
                    value,
                )
            })
        })
    }

    pub fn snapshot(&self) -> [SlotState<T>; N]
    where
        T: Clone,
    {
        self.slots.clone()
    }

    pub fn restore(slots: [SlotState<T>; N]) -> Result<Self, RestoreError> {
        if N > usize::from(u16::MAX) + 1 {
            return Err(RestoreError::TooManySlots);
        }
        for (index, slot) in slots.iter().enumerate() {
            if slot.generation == 0 {
                return Err(RestoreError::InvalidGeneration(index));
            }
            if slot.retired && (slot.generation != u32::MAX || slot.value.is_some()) {
                return Err(RestoreError::InvalidRetiredSlot(index));
            }
        }
        let len = slots.iter().filter(|slot| slot.value.is_some()).count();
        Ok(Self { slots, len })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_handle_cannot_touch_reused_slot_or_survive_reload() {
        let mut pool = EntityPool::<&str, 2>::new();
        let old = pool.insert("old tool").unwrap();
        assert_eq!(pool.remove(old), Some("old tool"));
        let new = pool.insert("new tool").unwrap();
        assert_eq!(old.index(), new.index());
        assert_ne!(old.generation(), new.generation());
        assert_eq!(pool.get_mut(old), None);
        assert_eq!(pool.remove(old), None);
        let restored = EntityPool::restore(pool.snapshot()).unwrap();
        assert_eq!(restored.get(old), None);
        assert_eq!(restored.get(new), Some(&"new tool"));
        assert_eq!(restored.len(), 1);
    }

    #[test]
    fn exhausted_generation_retires_slot_permanently() {
        let mut pool = EntityPool::<u8, 1>::restore([SlotState {
            generation: u32::MAX,
            retired: false,
            value: Some(7),
        }])
        .unwrap();
        let id = EntityId::from_parts(0, u32::MAX).unwrap();
        assert_eq!(pool.remove(id), Some(7));
        assert_eq!(pool.insert(9), Err(9));
        let mut restored = EntityPool::restore(pool.snapshot()).unwrap();
        assert_eq!(restored.insert(10), Err(10));
        assert!(restored.is_empty());
    }

    #[test]
    fn capacity_failure_returns_value_without_changing_pool() {
        let mut pool = EntityPool::<String, 1>::new();
        let id = pool.insert("reserved".into()).unwrap();
        let before = pool.clone();
        assert_eq!(pool.insert("not lost".into()), Err("not lost".into()));
        assert_eq!(pool, before);
        assert_eq!(pool.get(id).unwrap(), "reserved");
        assert!(EntityPool::<u8, 0>::new().insert(3).is_err());
    }

    #[test]
    fn corrupted_generations_and_retirement_are_rejected() {
        assert_eq!(
            EntityPool::<u8, 1>::restore([SlotState {
                generation: 0,
                retired: false,
                value: None,
            }]),
            Err(RestoreError::InvalidGeneration(0))
        );
        assert_eq!(
            EntityPool::<u8, 1>::restore([SlotState {
                generation: 1,
                retired: true,
                value: None,
            }]),
            Err(RestoreError::InvalidRetiredSlot(0))
        );
        assert!(EntityId::from_parts(0, 0).is_none());
        assert!(
            EntityPool::<u8, 1>::new()
                .get(EntityId::from_parts(99, 1).unwrap())
                .is_none()
        );
    }
}
